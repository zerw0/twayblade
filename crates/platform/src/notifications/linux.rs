use super::*;
use futures_util::StreamExt;
use std::collections::HashMap;
use zbus::{Connection, Proxy, zvariant::Value};

async fn bus() -> Result<&'static Connection, NotificationError> {
    static BUS: tokio::sync::OnceCell<Connection> = tokio::sync::OnceCell::const_new();
    BUS.get_or_try_init(|| async { Connection::session().await.map_err(|_| NotificationError) })
        .await
}
async fn proxy() -> Result<Proxy<'static>, NotificationError> {
    Proxy::new_owned(
        bus().await?.clone(),
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await
    .map_err(|_| NotificationError)
}
pub async fn permission() -> Permission {
    if proxy().await.is_ok() {
        Permission::Granted
    } else {
        Permission::Unavailable
    }
}
pub async fn request_permission() -> Permission {
    permission().await
}
pub async fn notify(
    _id: String,
    notification: Notification,
) -> Result<PendingNotification, NotificationError> {
    let proxy = proxy().await?;
    // Subscribe before sending so an immediate click cannot be lost.
    let mut actions = proxy
        .receive_signal("ActionInvoked")
        .await
        .map_err(|_| NotificationError)?;
    let mut closed = proxy
        .receive_signal("NotificationClosed")
        .await
        .map_err(|_| NotificationError)?;
    let mut hints = HashMap::new();
    hints.insert("suppress-sound", Value::Bool(!notification.sound));
    hints.insert("desktop-entry", Value::from("com.twayblade.desktop"));
    if notification.sound {
        hints.insert("sound-name", Value::from("message-new-instant"));
    }
    let body = super::escape_xml(&notification.body);
    let id: u32 = proxy
        .call(
            "Notify",
            &(
                "Twayblade",
                0u32,
                "",
                notification.title,
                body,
                vec!["default", notification.action_label],
                hints,
                -1i32,
            ),
        )
        .await
        .map_err(|_| NotificationError)?;
    Ok(PendingNotification {
        id: id.to_string(),
        response: Box::pin(async move {
            loop {
                tokio::select! {
                    action = actions.next() => {
                        let message = action?;
                        if let Ok((clicked, action)) = message.body().deserialize::<(u32, String)>() && clicked == id && action == "default" { return Some(notification.target); }
                    }
                    event = closed.next() => {
                        let message = event?;
                        if let Ok((closed, _)) = message.body().deserialize::<(u32, u32)>() && closed == id { return None; }
                    }
                }
            }
        }),
    })
}
pub async fn dismiss(id: &str) {
    if let Ok(id) = id.parse::<u32>()
        && let Ok(proxy) = proxy().await
    {
        let _: Result<(), _> = proxy.call("CloseNotification", &(id,)).await;
    }
}
pub async fn set_badge(count: u64) -> Result<(), NotificationError> {
    let mut properties = HashMap::new();
    properties.insert("count", Value::I64(count.min(i64::MAX as u64) as i64));
    properties.insert("count-visible", Value::Bool(count > 0));
    bus()
        .await?
        .emit_signal(
            None::<&str>,
            "/com/twayblade/desktop",
            "com.canonical.Unity.LauncherEntry",
            "Update",
            &("application://com.twayblade.desktop.desktop", properties),
        )
        .await
        .map_err(|_| NotificationError)
}
pub fn open_settings() -> Result<(), NotificationError> {
    Err(NotificationError)
}
