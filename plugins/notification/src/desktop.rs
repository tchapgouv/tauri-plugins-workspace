// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{
    ipc::Channel,
    plugin::{PermissionState, PluginApi},
    AppHandle, Manager, Runtime,
};

use crate::NotificationBuilder;

type EventListeners = Arc<Mutex<HashMap<String, Vec<Channel<serde_json::Value>>>>>;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionPerformedNotification {
    id: i32,
    title: Option<String>,
    body: Option<String>,
    action_type_id: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    extra: HashMap<String, serde_json::Value>,
}

#[allow(dead_code)]
fn emit_action_performed<R: Runtime>(
    app: &AppHandle<R>,
    action_id: &str,
    input_value: Option<String>,
    payload: &ActionPerformedNotification,
) -> crate::Result<()> {
    log::info!("[notification:backend] emit_action_performed called: action_id={}, notification_id={}", action_id, payload.id);
    let notification = app.state::<Notification<R>>().inner();
    let mut value = serde_json::to_value(payload)?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.insert("actionId".to_string(), action_id.into());
        if let Some(v) = input_value {
            map.insert("inputValue".to_string(), v.into());
        }
    }
    log::info!("[notification:backend] emitting to listeners: event=actionPerformed, value={}", value);
    notification.emit_to_listeners("actionPerformed", value);
    Ok(())
}

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<Notification<R>> {
    Ok(Notification {
        app: app.clone(),
        event_listeners: Arc::new(Mutex::new(HashMap::new())),
    })
}

/// Access to the notification APIs.
///
/// You can get an instance of this type via [`NotificationExt`](crate::NotificationExt)
pub struct Notification<R: Runtime> {
    app: AppHandle<R>,
    event_listeners: EventListeners,
}

impl<R: Runtime> crate::NotificationBuilder<R> {
    pub fn show(self) -> crate::Result<()> {
        let mut notification = imp::Notification::new(self.app.config().identifier.clone());

        if let Some(title) = self
            .data
            .title
            .as_ref()
            .or_else(|| self.app.config().product_name.as_ref())
        {
            notification = notification.title(title.clone());
        }
        if let Some(body) = self.data.body.as_ref() {
            notification = notification.body(body.clone());
        }
        if let Some(icon) = self.data.icon.as_ref() {
            notification = notification.icon(icon.clone());
        }
        if let Some(sound) = self.data.sound.as_ref() {
            notification = notification.sound(sound.clone());
        }

        #[cfg(not(feature = "windows7-compat"))]
        {
            log::info!("[notification:backend] showing notification with id: {}", self.data.id);
            let payload = ActionPerformedNotification {
                id: self.data.id,
                title: self.data.title.clone(),
                body: self.data.body.clone(),
                action_type_id: self.data.action_type_id.clone(),
                extra: self.data.extra.clone(),
            };
            let app_handle = self.app.clone();
            notification = notification.action_payload(payload).action_emitter(
                move |action_id, input_value, payload| {
                    log::info!("[notification:backend] action received from OS: id={}, action_id={:?}, input_value={:?}",
                        payload.id, action_id, input_value);
                    emit_action_performed(&app_handle, action_id, input_value, payload)
                },
            );
        }

        #[cfg(feature = "windows7-compat")]
        {
            notification.notify(&self.app)?;
        }
        #[cfg(not(feature = "windows7-compat"))]
        notification.show()?;

        Ok(())
    }
}

impl<R: Runtime> Notification<R> {
    pub fn builder(&self) -> NotificationBuilder<R> {
        NotificationBuilder::new(self.app.clone())
    }

    pub fn request_permission(&self) -> crate::Result<PermissionState> {
        Ok(PermissionState::Granted)
    }

    pub fn permission_state(&self) -> crate::Result<PermissionState> {
        Ok(PermissionState::Granted)
    }

    pub fn register_event_listener(&self, event: String, channel: Channel<serde_json::Value>) {
        if let Ok(mut guard) = self.event_listeners.lock() {
            guard.entry(event).or_default().push(channel);
        }
    }

    pub fn remove_event_listener(&self, event: &str, channel_id: u32) {
        if let Ok(mut guard) = self.event_listeners.lock() {
            if let Some(channels) = guard.get_mut(event) {
                channels.retain(|c| c.id() != channel_id);
            }
        }
    }

    #[allow(dead_code)]
    fn emit_to_listeners(&self, event: &str, payload: serde_json::Value) {
        log::info!("[notification:backend] emit_to_listeners called: event={}, payload={}", event, payload);
        if let Ok(guard) = self.event_listeners.lock() {
            let channel_count = guard.get(event).map(|c| c.len()).unwrap_or(0);
            log::info!("[notification:backend] found {} channels for event {}", channel_count, event);
            if let Some(channels) = guard.get(event) {
                for (idx, channel) in channels.clone().iter().enumerate() {
                    log::info!("[notification:backend] sending to channel {}/{}", idx + 1, channels.len());
                    let _ = channel.send(payload.clone());
                }
            }
        } else {
            log::warn!("[notification:backend] failed to lock event_listeners");
        }
    }
}

mod imp {
    //! Types and functions related to desktop notifications.

    #[cfg(windows)]
    use std::path::MAIN_SEPARATOR as SEP;

    type ActionEmitter = Box<
        dyn Fn(&str, Option<String>, &super::ActionPerformedNotification) -> crate::Result<()>
            + Send,
    >;

    /// The desktop notification definition.
    ///
    /// Allows you to construct a Notification data and send it.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use tauri_plugin_notification::NotificationExt;
    /// // first we build the application to access the Tauri configuration
    /// let app = tauri::Builder::default()
    ///   // on an actual app, remove the string argument
    ///   .build(tauri::generate_context!("test/tauri.conf.json"))
    ///   .expect("error while building tauri application");
    ///
    /// // shows a notification with the given title and body
    /// app.notification()
    ///   .builder()
    ///   .title("New message")
    ///   .body("You've got a new message.")
    ///   .show();
    ///
    /// // run the app
    /// app.run(|_app_handle, _event| {});
    /// ```
    #[allow(dead_code)]
    #[derive(Default)]
    pub struct Notification {
        /// The notification body.
        body: Option<String>,
        /// The notification title.
        title: Option<String>,
        /// The notification icon.
        icon: Option<String>,
        /// The notification sound.
        sound: Option<String>,
        /// The notification identifier
        identifier: String,
        /// Payload forwarded when an action is performed.
        action_payload: Option<super::ActionPerformedNotification>,
        /// Closure called when an action is performed.
        action_emitter: Option<ActionEmitter>,
    }

    impl std::fmt::Debug for Notification {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Notification")
                .field("body", &self.body)
                .field("title", &self.title)
                .field("icon", &self.icon)
                .field("sound", &self.sound)
                .field("identifier", &self.identifier)
                .field("action_payload", &self.action_payload)
                .field("action_emitter", &self.action_emitter.is_some())
                .finish()
        }
    }

    impl Notification {
        /// Initializes a instance of a Notification.
        pub fn new(identifier: impl Into<String>) -> Self {
            Self {
                identifier: identifier.into(),
                ..Default::default()
            }
        }

        /// Sets the notification body.
        #[must_use]
        pub fn body(mut self, body: impl Into<String>) -> Self {
            self.body = Some(body.into());
            self
        }

        /// Sets the notification title.
        #[must_use]
        pub fn title(mut self, title: impl Into<String>) -> Self {
            self.title = Some(title.into());
            self
        }

        /// Sets the notification icon.
        #[must_use]
        pub fn icon(mut self, icon: impl Into<String>) -> Self {
            self.icon = Some(icon.into());
            self
        }

        /// Sets the notification sound file.
        #[must_use]
        pub fn sound(mut self, sound: impl Into<String>) -> Self {
            self.sound = Some(sound.into());
            self
        }

        /// Sets the action payload.
        #[cfg(not(feature = "windows7-compat"))]
        #[must_use]
        pub fn action_payload(mut self, payload: super::ActionPerformedNotification) -> Self {
            self.action_payload = Some(payload);
            self
        }

        /// Sets the action emitter.
        #[cfg(not(feature = "windows7-compat"))]
        #[must_use]
        pub fn action_emitter(
            mut self,
            emitter: impl Fn(&str, Option<String>, &super::ActionPerformedNotification) -> crate::Result<()>
                + Send
                + 'static,
        ) -> Self {
            self.action_emitter = Some(ActionEmitter::from(Box::new(emitter)));
            self
        }

        /// Shows the notification.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use tauri_plugin_notification::NotificationExt;
        ///
        /// tauri::Builder::default()
        ///   .setup(|app| {
        ///     app.notification()
        ///       .builder()
        ///       .title("Tauri")
        ///       .body("Tauri is awesome!")
        ///       .show()
        ///       .unwrap();
        ///     Ok(())
        ///   })
        ///   .run(tauri::generate_context!("test/tauri.conf.json"))
        ///   .expect("error while running tauri application");
        /// ```
        ///
        /// ## Platform-specific
        ///
        /// - **Windows**: Not supported on Windows 7. If your app targets it, enable the `windows7-compat` feature and use [`Self::notify`].
        #[cfg_attr(
            all(not(docsrs), feature = "windows7-compat"),
            deprecated = "This function does not work on Windows 7. Use `Self::notify` instead."
        )]
        pub fn show(self) -> crate::Result<()> {
            let mut notification = notify_rust::Notification::new();
            if let Some(body) = self.body {
                notification.body(&body);
            }
            if let Some(title) = self.title {
                notification.summary(&title);
            }
            if let Some(icon) = self.icon {
                notification.icon(&icon);
            } else {
                notification.auto_icon();
            }
            if let Some(sound) = self.sound {
                notification.sound_name(&sound);
            }
            #[cfg(windows)]
            {
                let exe = tauri::utils::platform::current_exe()?;
                let exe_dir = exe.parent().expect("failed to get exe directory");
                let curr_dir = exe_dir.display().to_string();
                // set the notification's System.AppUserModel.ID only when running the installed app
                if !(curr_dir.ends_with(format!("{SEP}target{SEP}debug").as_str())
                    || curr_dir.ends_with(format!("{SEP}target{SEP}release").as_str()))
                {
                    notification.app_id(&self.identifier);
                }
            }
            #[cfg(target_os = "macos")]
            {
                let _ = notify_rust::set_application(if tauri::is_dev() {
                    "com.apple.Terminal"
                } else {
                    &self.identifier
                });
            }

            // XDG only: the server reports a body click only if a "default" action exists,
            // and per spec "default" is not rendered as a button. On macOS every declared
            // action becomes a visible button, and Windows reports body clicks natively.
            #[cfg(all(unix, not(target_os = "macos")))]
            notification.action("default", "");

            #[cfg(not(feature = "windows7-compat"))]
            {
                let payload = self.action_payload;
                let emitter = self.action_emitter;

                std::thread::spawn(move || {
                    eprintln!("[notification:backend] spawn: calling notification.show()...");
                    match notification.show() {
                        Ok(handle) => {
                            eprintln!("[notification:backend] spawn: show() returned handle, calling wait_for_response...");
                            let result = handle.wait_for_response(
                                |response: &notify_rust::NotificationResponse| {
                                    eprintln!("[notification:backend] wait_for_response callback fired: {:?}", response);
                                    let (action_id, input_value) = match response {
                                        notify_rust::NotificationResponse::Default => ("tap", None),
                                        notify_rust::NotificationResponse::Action(id) => {
                                            (id.as_str(), None)
                                        }
                                        notify_rust::NotificationResponse::Reply(text) => {
                                            ("tap", Some(text.clone()))
                                        }
                                        notify_rust::NotificationResponse::Closed(_) => {
                                            ("dismiss", None)
                                        }
                                    };
                                    eprintln!("[notification:backend] mapped response to action_id={}", action_id);
                                    if let (Some(p), Some(emit)) = (payload.as_ref(), emitter.as_ref())
                                    {
                                        if let Err(e) = emit(action_id, input_value, p) {
                                            eprintln!("[notification:backend] failed to emit actionPerformed: {e}");
                                        }
                                    }
                                },
                            );
                            eprintln!("[notification:backend] wait_for_response returned: {:?}", result);
                            if let Err(e) = result {
                                eprintln!("[notification:backend] failed to wait for notification response: {e}");
                            }
                        }
                        Err(e) => eprintln!("[notification:backend] failed to show notification: {e}"),
                    }
                });
            }

            #[cfg(feature = "windows7-compat")]
            {
                tauri::async_runtime::spawn(async move {
                    let _ = notification.show();
                });
            }

            Ok(())
        }

        /// Shows the notification. This API is similar to [`Self::show`], but it also works on Windows 7.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use tauri_plugin_notification::NotificationExt;
        ///
        /// tauri::Builder::default()
        ///   .setup(move |app| {
        ///     app.notification().builder()
        ///       .title("Tauri")
        ///       .body("Tauri is awesome!")
        ///       .show()
        ///       .unwrap();
        ///     Ok(())
        ///   })
        ///   .run(tauri::generate_context!("test/tauri.conf.json"))
        ///   .expect("error while running tauri application");
        /// ```
        #[cfg(feature = "windows7-compat")]
        #[cfg_attr(docsrs, doc(cfg(feature = "windows7-compat")))]
        #[allow(unused_variables)]
        pub fn notify<R: tauri::Runtime>(self, app: &tauri::AppHandle<R>) -> crate::Result<()> {
            #[cfg(windows)]
            {
                fn is_windows_7() -> bool {
                    let v = windows_version::OsVersion::current();
                    // windows 7 is 6.1
                    v.major == 6 && v.minor == 1
                }

                if is_windows_7() {
                    self.notify_win7(app)
                } else {
                    #[allow(deprecated)]
                    self.show()
                }
            }
            #[cfg(not(windows))]
            {
                #[allow(deprecated)]
                self.show()
            }
        }

        /// Shows the notification on Windows 7.
        #[cfg(all(windows, feature = "windows7-compat"))]
        fn notify_win7<R: tauri::Runtime>(self, app: &tauri::AppHandle<R>) -> crate::Result<()> {
            let app_ = app.clone();
            let _ = app.clone().run_on_main_thread(move || {
                let mut notification = win7_notifications::Notification::new();
                if let Some(body) = self.body {
                    notification.body(&body);
                }
                if let Some(title) = self.title {
                    notification.summary(&title);
                }
                if let Some(icon) = app_.default_window_icon() {
                    notification.icon(icon.rgba().to_vec(), icon.width(), icon.height());
                }
                let _ = notification.show();
            });

            Ok(())
        }
    }
}
