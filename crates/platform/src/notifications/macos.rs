use super::*;
use mac_usernotifications as un;

fn bundled() -> bool {
    un::check_bundle().is_ok()
}
pub async fn permission() -> Permission {
    if !bundled() {
        return Permission::Unavailable;
    }
    match un::get_notification_settings()
        .await
        .map(|settings| settings.authorization_status)
    {
        Ok(
            un::AuthorizationStatus::Authorized
            | un::AuthorizationStatus::Provisional
            | un::AuthorizationStatus::Ephemeral,
        ) => Permission::Granted,
        Ok(un::AuthorizationStatus::Denied) => Permission::Denied,
        Ok(un::AuthorizationStatus::NotDetermined) => Permission::NotRequested,
        _ => Permission::Unavailable,
    }
}
pub async fn request_permission() -> Permission {
    if !bundled() {
        return Permission::Unavailable;
    }
    let receiver = begin_permission_request();
    match receiver.await {
        Ok(true) => Permission::Granted,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unavailable,
    }
}
pub async fn notify(
    id: String,
    notification: Notification,
) -> Result<PendingNotification, NotificationError> {
    if !bundled() {
        return Err(NotificationError);
    }
    let content = un::Notification::new()
        .id(&id)
        .title(notification.title)
        .message(notification.body)
        .maybe_sound(notification.sound.then(|| un::sound::DEFAULT.to_owned()))
        .timeout(std::time::Duration::from_secs(24 * 60 * 60));
    let handle = content.send().await.map_err(|_| NotificationError)?;
    Ok(PendingNotification {
        id,
        response: Box::pin(async move {
            let response = handle.response().await.ok()?;
            response.is_default_action().then_some(notification.target)
        }),
    })
}
pub async fn dismiss(id: &str) {
    if bundled() {
        un::close_delivered(id).await;
    }
}
pub async fn set_badge(count: u64) -> Result<(), NotificationError> {
    if !bundled() {
        return Ok(());
    }
    objc2_user_notifications::UNUserNotificationCenter::currentNotificationCenter()
        .setBadgeCount_withCompletionHandler(count.min(isize::MAX as u64) as isize, None);
    Ok(())
}
pub fn open_settings() -> Result<(), NotificationError> {
    std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=com.matrixclient.desktop.development")
        .spawn().map(|_| ()).map_err(|_| NotificationError)
}

fn begin_permission_request() -> tokio::sync::oneshot::Receiver<bool> {
    use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Mutex::new(Some(sender));
    let callback = block2::RcBlock::new(
        move |granted: objc2::runtime::Bool, _error: *mut objc2_foundation::NSError| {
            if let Ok(mut sender) = sender.lock()
                && let Some(sender) = sender.take()
            {
                let _ = sender.send(granted.as_bool());
            }
        },
    );
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge,
            &callback,
        );
    // Drop the non-Send block before awaiting; the OS retains its own copy.
    drop(callback);
    receiver
}
