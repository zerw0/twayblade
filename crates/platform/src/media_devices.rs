//! Bounded hot-plug notifications. macOS uses CoreAudio/AVFoundation callbacks;
//! other desktop targets refresh no more often than every two seconds.
#[cfg(not(target_os = "macos"))]
use tokio::sync::watch;
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "CoreAudio property listener ABI and retained AVFoundation observers"
)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{changes, default_audio};
#[cfg(not(target_os = "macos"))]
pub fn changes() -> watch::Receiver<u64> {
    let (send, receive) = watch::channel(0);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            if send.is_closed() {
                break;
            }
            send.send_modify(|n| *n = n.wrapping_add(1));
        }
    });
    receive
}
#[cfg(not(target_os = "macos"))]
pub fn default_audio(_input: bool) -> Option<String> {
    None
}
