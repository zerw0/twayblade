use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use tokio::sync::watch;
pub(super) fn read() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}
pub(super) fn observe(sender: watch::Sender<bool>) {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let callback_sender = sender.clone();
    let callback = block2::RcBlock::new(
        move |_notification: std::ptr::NonNull<objc2_foundation::NSNotification>| {
            callback_sender.send_if_modified(|value| {
                let next = read();
                let changed = *value != next;
                *value = next;
                changed
            });
        },
    );
    // SAFETY: no object filter or queue is supplied. The callback captures only
    // a Send+Sync watch sender and never dereferences the notification pointer.
    // Center, block and observer are retained together for this process worker.
    let _observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
            None,
            None,
            &callback,
        )
    };
    sender.send_replace(read());
    loop {
        std::thread::park();
    }
}
