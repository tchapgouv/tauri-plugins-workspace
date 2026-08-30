// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{
    ipc::Channel,
    plugin::{PermissionState, PluginApi},
    AppHandle, Runtime,
};

use crate::{models::ActionType, NotificationBuilder, NotificationExt};

/// Registered plugin event listeners, keyed by event name.
/// Each event maps to the list of JS channels to notify when the event fires.
type EventListeners = Arc<Mutex<std::collections::HashMap<String, Vec<Channel<serde_json::Value>>>>>;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<Notification<R>> {
    Ok(Notification::new(app.clone()))
}

/// Access to the notification APIs.
///
/// You can get an instance of this type via [`NotificationExt`](crate::NotificationExt)
pub struct Notification<R: Runtime> {
    app: AppHandle<R>,
    action_types: Arc<Mutex<std::collections::HashMap<String, ActionType>>>,
    event_listeners: EventListeners,
}

impl<R: Runtime> Notification<R> {
    fn new(app: AppHandle<R>) -> Self {
        Self {
            app,
            action_types: Arc::new(Mutex::new(std::collections::HashMap::new())),
            event_listeners: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Registers a listener channel for the given event name (used by `register_listener`).
    pub fn register_event_listener(&self, event: String, channel: Channel<serde_json::Value>) {
        self.event_listeners
            .lock()
            .unwrap()
            .entry(event)
            .or_default()
            .push(channel);
    }

    /// Removes the listener channel with the given id from the given event.
    pub fn remove_event_listener(&self, event: &str, channel_id: u32) {
        if let Some(channels) = self.event_listeners.lock().unwrap().get_mut(event) {
            channels.retain(|c| c.id() != channel_id);
        }
    }

    /// Emits the given event payload to all registered listeners of this event.
    fn emit_to_listeners(&self, event: &str, payload: serde_json::Value) {
        if let Some(channels) = self.event_listeners.lock().unwrap().get(event) {
            for channel in channels {
                let _ = channel.send(payload.clone());
            }
        }
    }
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
        if let Some(ref body) = self.data.body {
            notification = notification.body(body.clone());
        }
        if let Some(icon) = self.data.icon {
            notification = notification.icon(icon);
        }
        if let Some(sound) = self.data.sound {
            notification = notification.sound(sound);
        }

        // Resolve registered action types (desktop support for registerActionTypes)
        if let Some(action_type_id) = self.data.action_type_id.as_deref() {
            let action = self
                .app
                .notification()
                .action_types
                .lock()
                .unwrap()
                .get(action_type_id)
                .cloned();
            if let Some(action_type) = action {
                let actions: Vec<_> = action_type
                    .actions()
                    .iter()
                    .map(|a| (a.id().to_string(), a.title().to_string()))
                    .collect();
                notification = notification.actions(actions);
            }
        }

        #[cfg(feature = "windows7-compat")]
        {
            notification.notify(&self.app)?;
        }
        #[cfg(not(feature = "windows7-compat"))]
        {
            // merge stored action payload for the event emitted from the waiter thread
            let payload = ActionPerformedNotification {
                id: self.data.id,
                title: self.data.title.clone(),
                body: self.data.body.clone(),
                action_type_id: self.data.action_type_id.clone(),
            };
            notification = notification.action_payload(payload);
            let app_handle = self.app.clone();
            notification = notification.action_emitter(move |action_id, payload| {
                emit_action_performed(&app_handle, action_id, &payload)
            });
            notification.show()?;
        }

        Ok(())
    }
}

/// Payload emitted on `plugin:notification://actionPerformed` from the desktop backend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionPerformedNotification {
    id: i32,
    title: Option<String>,
    body: Option<String>,
    action_type_id: Option<String>,
}

/// Emits the `actionPerformed` event to all registered plugin listeners
/// (the channels registered through the `register_listener` command).
///#[cfg_attr(feature = "windows7-compat", allow(dead_code))]
fn emit_action_performed<R: Runtime>(
    app: &AppHandle<R>,
    action_id: &str,
    notification: &ActionPerformedNotification,
) -> crate::Result<()> {
    // Same normalization as iOS: "default" (notify-rust body click) -> "tap"
    let action_id = if action_id == "default" {
        "tap"
    } else {
        action_id
    };

    #[derive(Serialize, Clone)]
    #[serde(rename_all = "camelCase")]
    struct Payload<'a> {
        action_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_value: Option<String>,
        notification: &'a ActionPerformedNotification,
    }

    let payload = Payload {
        action_id,
        input_value: None,
        notification,
    };

    let value = serde_json::to_value(payload)?;
    app.notification().emit_to_listeners("actionPerformed", value);
    Ok(())
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

    /// Registers action types for use with `actionTypeId` on desktop.
    pub fn register_action_types(&self, types: Vec<ActionType>) -> crate::Result<()> {
        let mut map = self.action_types.lock().unwrap();
        for action_type in types {
            map.insert(action_type.id().to_string(), action_type);
        }
        Ok(())
    }
}

mod imp {
    //! Types and functions related to desktop notifications.

    #[cfg(windows)]
    use std::path::MAIN_SEPARATOR as SEP;

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
        /// Registered desktop actions as (id, title) pairs.
        actions: Vec<(String, String)>,
        /// Payload to include on `actionPerformed` events.
        action_payload: Option<crate::desktop::ActionPerformedNotification>,
        /// Optional emitter used by the waiter thread to emit the `actionPerformed` event.
        action_emitter: Option<
            Box<dyn Fn(&str, &crate::desktop::ActionPerformedNotification) -> crate::Result<()> + Send>,
        >,
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

        /// Sets the desktop actions to attach to this notification.
        #[must_use]
        pub fn actions(mut self, actions: Vec<(String, String)>) -> Self {
            self.actions = actions;
            self
        }

        /// Sets the payload emitted when an action is performed on this notification.
        #[must_use]
        ///#[cfg_attr(feature = "windows7-compat", allow(dead_code))]
        pub fn action_payload(
            mut self,
            payload: crate::desktop::ActionPerformedNotification,
        ) -> Self {
            self.action_payload = Some(payload);
            self
        }

        /// Sets the emitter used to emit the `actionPerformed` event from the waiter thread.
        #[must_use]
        ///#[cfg(not(feature = "windows7-compat"))]
        pub fn action_emitter(
            mut self,
            emitter: impl Fn(&str, &crate::desktop::ActionPerformedNotification) -> crate::Result<()> + Send + 'static,
        ) -> Self {
            self.action_emitter = Some(Box::new(emitter));
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

            // Add desktop actions (registerActionTypes)
            for (action_id, action_title) in &self.actions {
                notification.action(action_id, action_title);
            }

          
            let payload = self.action_payload;

            // `wait_for_action` is only available on platforms where notify-rust
            // returns a `NotificationHandle`
            std::thread::spawn(move || {
                match notification.show() {
                    Ok(handle) => {
                        handle.wait_for_action(|action_id| {
                            if let (Some(payload), Some(emitter)) = (payload, &self.action_emitter) {
                                let _ = emitter(action_id, &payload);
                            }
                        });
                    }
                    Err(e) => log::error!("failed to show notification: {e}"),
                }
            });

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
