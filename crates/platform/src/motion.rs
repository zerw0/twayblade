//! Process-wide OS motion preference and bounded latest-value notifications.
use std::sync::OnceLock;
use tokio::sync::watch;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "Audited NSNotificationCenter registration; no pointers escape this module"
)]
mod macos;
#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "SPI_GETCLIENTAREAANIMATION requires a correctly sized output pointer"
)]
mod windows;
#[cfg(target_os = "linux")]
use linux as native;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(target_os = "windows")]
use windows as native;
fn channel() -> &'static watch::Sender<bool> {
    static CHANNEL: OnceLock<watch::Sender<bool>> = OnceLock::new();
    CHANNEL.get_or_init(|| {
        let sender = watch::channel(native::read()).0;
        let worker = sender.clone();
        let _ = std::thread::Builder::new()
            .name("motion-preference".into())
            .spawn(move || native::observe(worker));
        sender
    })
}
pub fn reduce_motion() -> bool {
    *channel().borrow()
}
pub fn reduce_motion_changes() -> watch::Receiver<bool> {
    channel().subscribe()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_and_subscribe_share_one_process_channel() {
        let first = reduce_motion_changes();
        let second = reduce_motion_changes();
        assert!(first.same_channel(&second));
        let _ = reduce_motion();
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn published_preference_matches_the_native_workspace_read() {
        assert_eq!(reduce_motion(), native::read());
    }
}
