use super::*;
use ::windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::TypedEventHandler,
    UI::Notifications::*,
    core::{HSTRING, IInspectable},
};
const APP_ID: &str = "com.twayblade.desktop";
fn notifier() -> ::windows::core::Result<ToastNotifier> {
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
}
pub async fn permission() -> Permission {
    match notifier().and_then(|notifier| notifier.Setting()) {
        Ok(NotificationSetting::Enabled) => Permission::Granted,
        Ok(_) => Permission::Denied,
        Err(_) => Permission::Unavailable,
    }
}
pub async fn request_permission() -> Permission {
    permission().await
}
pub async fn notify(
    id: String,
    notification: Notification,
) -> Result<PendingNotification, NotificationError> {
    let build = || -> ::windows::core::Result<_> {
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(format!("<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>{}</toast>", super::escape_xml(&notification.title), super::escape_xml(&notification.body), if notification.sound { "" } else { "<audio silent=\"true\"/>" })))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        toast.SetTag(&HSTRING::from(&id))?;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        let click = sender.clone();
        let activated = toast.Activated(
            &TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, _| {
                if let Ok(mut sender) = click.lock()
                    && let Some(sender) = sender.take()
                {
                    let _ = sender.send(true);
                }
                Ok(())
            }),
        )?;
        let dismissed = toast.Dismissed(&TypedEventHandler::<
            ToastNotification,
            ToastDismissedEventArgs,
        >::new(move |_, _| {
            if let Ok(mut sender) = sender.lock()
                && let Some(sender) = sender.take()
            {
                let _ = sender.send(false);
            }
            Ok(())
        }))?;
        notifier()?.Show(&toast)?;
        Ok((toast, activated, dismissed, receiver))
    };
    let (toast, activated, dismissed, receiver) = build().map_err(|_| NotificationError)?;
    Ok(PendingNotification {
        id,
        response: Box::pin(async move {
            let clicked = receiver.await.unwrap_or(false);
            let _ = toast.RemoveActivated(activated);
            let _ = toast.RemoveDismissed(dismissed);
            clicked.then_some(notification.target)
        }),
    })
}
pub async fn dismiss(id: &str) {
    if let Ok(history) = ToastNotificationManager::History() {
        let _ = history.RemoveGroupedTagWithId(
            &HSTRING::from(id),
            &HSTRING::new(),
            &HSTRING::from(APP_ID),
        );
    }
}
pub async fn set_badge(count: u64) -> Result<(), NotificationError> {
    let update = || -> ::windows::core::Result<()> {
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(format!("<badge value=\"{count}\"/>")))?;
        let badge = BadgeNotification::CreateBadgeNotification(&xml)?;
        BadgeUpdateManager::CreateBadgeUpdaterForApplicationWithId(&HSTRING::from(APP_ID))?
            .Update(&badge)
    };
    update().map_err(|_| NotificationError)
}
pub fn open_settings() -> Result<(), NotificationError> {
    std::process::Command::new("explorer.exe")
        .arg("ms-settings:notifications")
        .spawn()
        .map(|_| ())
        .map_err(|_| NotificationError)
}
