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
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::audio_defaults;
#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "Balanced COM initialization and owned IMMDevice endpoint IDs"
)]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::audio_defaults;

#[derive(Clone, PartialEq, Eq)]
pub struct DefaultAudioDevice {
    pub id: String,
    /// PulseAudio's description lets core match SDK devices that expose only
    /// enumeration indices. Neither field is logged or persisted here.
    pub name: Option<String>,
}
impl std::fmt::Debug for DefaultAudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultAudioDevice").finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioDefaults {
    pub input: Option<DefaultAudioDevice>,
    pub output: Option<DefaultAudioDevice>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DefaultAudioError {
    #[error("audio_defaults_busy")]
    Busy,
    #[error("audio_defaults_unavailable")]
    Unavailable,
    #[error("audio_defaults_timeout")]
    Timeout,
    #[error("audio_default_identifier_invalid")]
    InvalidIdentifier,
}
/// Bounds both waiting time and the number of native queries left running if an
/// OS service stalls. The worker owns its permit until the OS call returns.
pub async fn query_audio_defaults() -> Result<AudioDefaults, DefaultAudioError> {
    use std::sync::{Arc, OnceLock};
    static QUERY: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let permit = QUERY
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| DefaultAudioError::Busy)?;
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            audio_defaults()
        }),
    )
    .await
    .map_err(|_| DefaultAudioError::Timeout)?
    .map_err(|_| DefaultAudioError::Unavailable)?
}
/// Synchronous OS metadata query. Call on a blocking worker, not a UI thread.
#[cfg(target_os = "macos")]
pub fn audio_defaults() -> Result<AudioDefaults, DefaultAudioError> {
    let device =
        |input| macos::default_audio(input).map(|id| DefaultAudioDevice { id, name: None });
    Ok(AudioDefaults {
        input: device(true),
        output: device(false),
    })
}
#[cfg(not(target_os = "macos"))]
pub fn changes() -> watch::Receiver<u64> {
    let (send, receive) = watch::channel(0u64);
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
pub fn default_audio(input: bool) -> Option<String> {
    let defaults = audio_defaults().ok()?;
    (if input {
        defaults.input
    } else {
        defaults.output
    })
    .map(|device| device.id)
}
