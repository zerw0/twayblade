//! OS permission prompts, separate from capture/media and frontend wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturePermission {
    Microphone,
    Camera,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PermissionError {
    #[error("media_permission_denied")]
    Denied(CapturePermission),
    #[error("media_permission_restricted")]
    Restricted(CapturePermission),
    #[error("media_permission_unsupported")]
    Unsupported,
}
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "AVFoundation permission API; no raw media or object pointers escape"
)]
mod macos {
    use super::*;
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo,
    };
    pub async fn request(kind: CapturePermission) -> Result<(), PermissionError> {
        // SAFETY: AVFoundation exports immutable NSString constants. These APIs
        // accept retained blocks; the callback captures only a thread-safe sender.
        let media = unsafe {
            match kind {
                CapturePermission::Microphone => AVMediaTypeAudio,
                CapturePermission::Camera => AVMediaTypeVideo,
            }
        }
        .ok_or(PermissionError::Unsupported)?;
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) } {
            AVAuthorizationStatus::Authorized => return Ok(()),
            AVAuthorizationStatus::Denied => return Err(PermissionError::Denied(kind)),
            AVAuthorizationStatus::Restricted => return Err(PermissionError::Restricted(kind)),
            _ => {}
        }
        let (tx, rx) = tokio::sync::watch::channel(None);
        {
            let callback = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
                tx.send_replace(Some(granted.as_bool()));
            });
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &callback)
            };
        }
        let mut rx = rx;
        while rx.borrow().is_none() {
            rx.changed()
                .await
                .map_err(|_| PermissionError::Unsupported)?;
        }
        let granted = (*rx.borrow()).unwrap_or(false);
        if granted {
            Ok(())
        } else {
            Err(PermissionError::Denied(kind))
        }
    }
}
pub async fn request(kind: CapturePermission) -> Result<(), PermissionError> {
    #[cfg(target_os = "macos")]
    {
        macos::request(kind).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Err(PermissionError::Unsupported)
    }
}
