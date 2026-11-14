//! WinRT initialization lives on the thread that owns the SDK capturer.
#![allow(
    unsafe_code,
    reason = "Balanced Windows Runtime initialization on the capture thread"
)]
use super::{ScreenBackend, ScreenError};
use ::windows::{
    Graphics::Capture::GraphicsCaptureSession,
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
};

pub(super) struct Runtime(std::marker::PhantomData<std::rc::Rc<()>>);
impl Runtime {
    pub fn new() -> Result<Self, ScreenError> {
        // A dedicated capture thread is initialized as MTA. The guard is created
        // before the capturer and dropped after it, including every error path.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|_| ScreenError::BackendUnavailable(ScreenBackend::WindowsGraphicsCapture))?;
        let guard = Self(Default::default());
        if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
            return Err(ScreenError::BackendUnavailable(
                ScreenBackend::WindowsGraphicsCapture,
            ));
        }
        Ok(guard)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}
