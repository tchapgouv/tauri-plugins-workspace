// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use tauri::{command, ipc::Channel, plugin::PermissionState, AppHandle, Runtime, State};

use crate::{models::ActionType, ActiveNotification, Notification, NotificationData, Result};

#[command]
pub(crate) async fn is_permission_granted<R: Runtime>(
    _app: AppHandle<R>,
    notification: State<'_, Notification<R>>,
) -> Result<Option<bool>> {
    let state = notification.permission_state()?;
    match state {
        PermissionState::Granted => Ok(Some(true)),
        PermissionState::Denied => Ok(Some(false)),
        PermissionState::Prompt | PermissionState::PromptWithRationale => Ok(None),
    }
}

#[command]
pub(crate) async fn request_permission<R: Runtime>(
    _app: AppHandle<R>,
    notification: State<'_, Notification<R>>,
) -> Result<PermissionState> {
    notification.request_permission()
}

#[command]
pub(crate) async fn notify<R: Runtime>(
    _app: AppHandle<R>,
    notification: State<'_, Notification<R>>,
    options: NotificationData,
) -> Result<()> {
    let mut builder = notification.builder();
    builder.data = options;
    builder.show()
}

#[cfg(desktop)]
#[command]
pub(crate) async fn register_action_types<R: Runtime>(
    _app: AppHandle<R>,
    notification: State<'_, Notification<R>>,
    types: Vec<ActionType>,
) -> Result<()> {
    notification.register_action_types(types)
}

#[cfg(desktop)]
#[command]
pub(crate) async fn register_listener<R: Runtime>(
    _app: AppHandle<R>,
    notification: State<'_, Notification<R>>,
    event: String,
    handler: Channel<serde_json::Value>,
) -> Result<()> {
    notification.register_event_listener(event, handler);
    Ok(())
}

#[cfg(desktop)]
#[command]
pub(crate) async fn remove_listener<R: Runtime>(
    _app: AppHandle<R>,
    notification: State<'_, Notification<R>>,
    event: String,
    channel_id: u32,
) -> Result<()> {
    notification.remove_event_listener(&event, channel_id);
    Ok(())
}

#[cfg(desktop)]
#[command]
pub(crate) async fn get_active<R: Runtime>(
    _app: AppHandle<R>,
    _notification: State<'_, Notification<R>>,
) -> Result<Vec<ActiveNotification>> {
    Ok(Vec::new())
}

#[cfg(desktop)]
#[command]
pub(crate) async fn remove_active<R: Runtime>(
    _app: AppHandle<R>,
    _notification: State<'_, Notification<R>>,
    #[allow(unused_variables)] notifications: Option<Vec<i32>>,
) -> Result<()> {
    Ok(())
}
