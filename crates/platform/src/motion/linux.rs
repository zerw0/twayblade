use ashpd::desktop::settings::{ReducedMotion, Settings};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::watch;
pub(super) fn read() -> bool {
    false
}
async fn preference(settings: &Settings) -> bool {
    if let Ok(Ok(value)) =
        tokio::time::timeout(Duration::from_secs(2), settings.reduced_motion()).await
    {
        return value == ReducedMotion::ReducedMotion;
    }
    for (namespace, key) in [
        ("org.gnome.desktop.interface", "enable-animations"),
        ("org.gtk.Settings.Interface", "gtk-enable-animations"),
    ] {
        if let Ok(Ok(enabled)) = tokio::time::timeout(
            Duration::from_secs(2),
            settings.read::<bool>(namespace, key),
        )
        .await
        {
            return !enabled;
        }
    }
    false
}
pub(super) fn observe(sender: watch::Sender<bool>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    runtime.block_on(async {
        loop {
            let connection = tokio::time::timeout(Duration::from_secs(2),ashpd::zbus::Connection::session()).await;
            if let Ok(Ok(connection)) = connection {
                if let Ok(Ok(settings)) = tokio::time::timeout(Duration::from_secs(2),Settings::with_connection(connection.clone())).await {
                    let changes = tokio::time::timeout(Duration::from_secs(2),settings.receive_setting_changed()).await;
                    sender.send_replace(preference(&settings).await);
                    if let Ok(Ok(changes)) = changes {
                        tokio::pin!(changes);
                        loop { tokio::select! {
                            change = changes.next() => { let Some(change)=change else { break; }; if matches!(change.namespace(), "org.freedesktop.appearance"|"org.gnome.desktop.interface"|"org.gtk.Settings.Interface") { let next=preference(&settings).await; sender.send_if_modified(|current| { let changed=*current!=next; *current=next; changed }); } }
                            _ = connection.closed() => break,
                        } }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}
