//! Native camera capture; only macOS is implemented in phase 1.
use crate::{CaptureSource, Device, MediaError};
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "AVFoundation delegate and bounded CoreVideo pixel-buffer read"
)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{Camera, devices};
#[cfg(not(target_os = "macos"))]
pub fn devices() -> Vec<Device> {
    Vec::new()
}
#[cfg(not(target_os = "macos"))]
pub struct Camera { _private: () }
#[cfg(not(target_os = "macos"))]
impl Camera {
    pub async fn start(_id: &str) -> Result<Self, MediaError> {
        Err(MediaError::DeviceUnavailable)
    }
}
#[cfg(not(target_os = "macos"))]
impl CaptureSource for Camera {
    fn source(&self) -> livekit::webrtc::video_source::RtcVideoSource {
        unreachable!("unsupported capture cannot be constructed")
    }
    fn stop(&mut self) {}
}
