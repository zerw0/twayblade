//! Native notifications. Titles and bodies deliberately have no Debug output.
use futures_util::future::BoxFuture;
mod activation;
pub use activation::{
    LaunchActivation, acknowledge_launch, clear_launch_activation, initialize_activation,
    launch_activations,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "Audited native UserNotifications delegate and callback types"
)]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux as native;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(target_os = "windows")]
use windows as native;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Permission {
    #[default]
    Unknown,
    NotRequested,
    Granted,
    Denied,
    Unavailable,
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Activation {
    pub account: u128,
    pub room: String,
    pub thread: Option<String>,
}
pub struct Notification {
    pub title: String,
    pub body: String,
    pub sound: bool,
    /// A PNG shown with the notification (the sender's or room's avatar).
    pub image: Option<Vec<u8>>,
    pub action_label: &'static str,
    pub target: Activation,
    pub launch: Option<LaunchActivation>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum NotificationAction {
    #[default]
    Open,
    Accept,
    Decline,
}
pub struct NotificationResponse {
    pub target: Activation,
    pub action: NotificationAction,
}
pub struct PendingNotification {
    pub id: String,
    pub response: BoxFuture<'static, Option<NotificationResponse>>,
}
#[derive(Debug, thiserror::Error)]
#[error("native_notification_unavailable")]
pub struct NotificationError;

pub async fn permission() -> Permission {
    native::permission().await
}
pub async fn request_permission() -> Permission {
    native::request_permission().await
}
pub async fn notify(notification: Notification) -> Result<PendingNotification, NotificationError> {
    let id = format!("{:016x}", uuid::Uuid::new_v4().as_u64_pair().0);
    native::notify(id, notification, false).await
}
/// Calls expose native actions where implemented; OS Focus/DND owns presentation.
pub async fn notify_call(
    notification: Notification,
) -> Result<PendingNotification, NotificationError> {
    let id = format!("{:016x}", uuid::Uuid::new_v4().as_u64_pair().0);
    native::notify(id, notification, true).await
}
pub async fn dismiss(id: &str) {
    native::dismiss(id).await;
}
pub async fn set_badge(count: u64) -> Result<(), NotificationError> {
    native::set_badge(count).await
}
pub fn open_settings() -> Result<(), NotificationError> {
    native::open_settings()
}

/// Writes a notification image where the system can read it. Files older
/// than a day are removed; the system has shown or dropped them by then
/// (macOS moves its copy into its own store).
fn image_file(png: &[u8]) -> Option<std::path::PathBuf> {
    let directory = directories::ProjectDirs::from("", "Twayblade", "Twayblade")?
        .cache_dir()
        .join("notification-images");
    std::fs::create_dir_all(&directory).ok()?;
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > std::time::Duration::from_secs(24 * 60 * 60));
            if old {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let path = directory.join(format!("{}.png", uuid::Uuid::new_v4().simple()));
    std::fs::write(&path, png).ok()?;
    Some(path)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unbundled_runs_do_not_request_permission_or_deliver() {
        assert_eq!(permission().await, Permission::Unavailable);
        assert_eq!(request_permission().await, Permission::Unavailable);
        assert!(
            notify(Notification {
                title: "Sample".into(),
                body: "Sample".into(),
                sound: false,
                image: None,
                action_label: "Open",
                target: Default::default(),
                launch: None,
            })
            .await
            .is_err()
        );
        assert!(set_badge(0).await.is_ok());
    }
}
