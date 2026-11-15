//! Native camera capture: AVFoundation on macOS, Video4Linux2 on Linux and
//! Media Foundation on Windows.
use crate::{CaptureSource, Device, MediaError};
mod convert;
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "AVFoundation delegate and bounded CoreVideo pixel-buffer read"
)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{Camera, default_device, devices};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{Camera, default_device, devices};
#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "Media Foundation COM calls and a bounded sample-buffer read on the capture thread"
)]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{Camera, default_device, devices};
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod unsupported {
    use super::*;
    pub fn devices() -> Vec<Device> {
        Vec::new()
    }
    pub fn default_device() -> Option<String> {
        None
    }
    pub struct Camera {
        _private: (),
    }
    impl Camera {
        pub async fn start(_id: &str) -> Result<Self, MediaError> {
            Err(MediaError::DeviceUnavailable)
        }
    }
    impl CaptureSource for Camera {
        fn source(&self) -> livekit::webrtc::video_source::RtcVideoSource {
            unreachable!("unsupported capture cannot be constructed")
        }
        fn stop(&mut self) {}
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub use unsupported::{Camera, default_device, devices};
