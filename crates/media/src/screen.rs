//! Screen sharing through libwebrtc's desktop capturer. macOS shows the
//! ScreenCaptureKit system picker and Linux the desktop portal (PipeWire on
//! Wayland), so the person chooses a screen or window outside the app; Windows
//! shares the first screen. One capture thread owns the capturer, converts
//! frames to I420 (scaled to fit `MAX_*`) and feeds a screencast source.
use crate::MediaError;
use livekit::webrtc::{
    desktop_capturer::{
        CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
        DesktopFrame,
    },
    native::yuv_helper,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

const MAX_WIDTH: u32 = 1920;
const MAX_HEIGHT: u32 = 1080;
const FRAME_INTERVAL: Duration = Duration::from_millis(66);
/// A picker left open this long without a choice ends the attempt.
const CHOOSE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenStatus {
    /// The first frame arrived: a screen or window was chosen.
    Live,
    /// The picker was cancelled or timed out, the system stopped sharing
    /// (for example from the menu bar), or capture failed.
    Ended,
}

pub struct ScreenCapture {
    source: NativeVideoSource,
    stop: Arc<AtomicBool>,
}
impl ScreenCapture {
    /// Starts capture and, where the platform has one, the system picker.
    /// Status changes arrive on `events`; `Ended` is always sent last.
    pub fn start(events: mpsc::Sender<ScreenStatus>) -> Result<Self, MediaError> {
        let source = NativeVideoSource::new(
            VideoResolution {
                width: MAX_WIDTH,
                height: MAX_HEIGHT,
            },
            true,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let feed = source.clone();
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("twayblade-screen".into())
            .spawn(move || {
                // The capturer is not Send: create, drive and drop it here.
                let Some((mut capturer, choice)) = capturer() else {
                    let _ = ready_tx.send(Err(MediaError::DeviceUnavailable));
                    return;
                };
                let _ = ready_tx.send(Ok(()));
                let live = Arc::new(AtomicBool::new(false));
                let ended = Arc::new(AtomicBool::new(false));
                let (frame_live, frame_ended, frame_events) =
                    (live.clone(), ended.clone(), events.clone());
                let clock = Instant::now();
                capturer.start_capture(choice, move |result| match result {
                    Ok(frame) => {
                        if push(&feed, &frame, clock) && !frame_live.swap(true, Ordering::Relaxed) {
                            let _ = frame_events.try_send(ScreenStatus::Live);
                        }
                    }
                    // The picker is still open or a frame was skipped.
                    Err(CaptureError::Temporary) => {}
                    Err(CaptureError::Permanent) => frame_ended.store(true, Ordering::Relaxed),
                });
                while !stopped.load(Ordering::Relaxed)
                    && !ended.load(Ordering::Relaxed)
                    && (live.load(Ordering::Relaxed) || clock.elapsed() < CHOOSE_TIMEOUT)
                {
                    capturer.capture_frame();
                    std::thread::sleep(FRAME_INTERVAL);
                }
                drop(capturer);
                let _ = events.try_send(ScreenStatus::Ended);
            })
            .map_err(|_| MediaError::DeviceUnavailable)?;
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| MediaError::DeviceUnavailable)??;
        Ok(Self { source, stop })
    }
    pub fn source(&self) -> RtcVideoSource {
        RtcVideoSource::Native(self.source.clone())
    }
}
impl Drop for ScreenCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

type Choice = Option<livekit::webrtc::desktop_capturer::CaptureSource>;
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn capturer() -> Option<(DesktopCapturer, Choice)> {
    let mut options = DesktopCapturerOptions::new(DesktopCaptureSourceType::Generic);
    options.set_include_cursor(true);
    Some((DesktopCapturer::new(options)?, None))
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn capturer() -> Option<(DesktopCapturer, Choice)> {
    let mut options = DesktopCapturerOptions::new(DesktopCaptureSourceType::Screen);
    options.set_include_cursor(true);
    let capturer = DesktopCapturer::new(options)?;
    let first = capturer.get_source_list().into_iter().next()?;
    Some((capturer, Some(first)))
}

/// Fits `width`×`height` inside the maximum, keeping the aspect ratio and
/// even dimensions for I420.
fn fitted(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width as u64, height as u64);
    let (w, h) = if w <= MAX_WIDTH as u64 && h <= MAX_HEIGHT as u64 {
        (w, h)
    } else if w * MAX_HEIGHT as u64 >= h * MAX_WIDTH as u64 {
        (MAX_WIDTH as u64, h * MAX_WIDTH as u64 / w)
    } else {
        (w * MAX_HEIGHT as u64 / h, MAX_HEIGHT as u64)
    };
    let even = |v: u64| ((v as u32) & !1).max(2);
    (even(w), even(h))
}

fn push(source: &NativeVideoSource, frame: &DesktopFrame, clock: Instant) -> bool {
    let (width, height) = (frame.width(), frame.height());
    if width <= 0 || height <= 0 || width > 16_384 || height > 16_384 {
        return false;
    }
    let (width, height) = (width as u32, height as u32);
    let stride = frame.stride();
    let data = frame.data();
    if stride < width * 4 || data.len() < stride as usize * height as usize {
        return false;
    }
    let mut buffer = I420Buffer::new(width, height);
    let (sy, su, sv) = buffer.strides();
    let (y, u, v) = buffer.data_mut();
    // libyuv ARGB is BGRA in memory, the desktop capturer's layout.
    yuv_helper::argb_to_i420(
        data,
        stride,
        y,
        sy,
        u,
        su,
        v,
        sv,
        width as i32,
        height as i32,
    );
    let (fit_width, fit_height) = fitted(width, height);
    let buffer = if (fit_width, fit_height) == (width, height) {
        buffer
    } else {
        buffer.scale(fit_width as i32, fit_height as i32)
    };
    let mut frame = VideoFrame::new(VideoRotation::VideoRotation0, buffer);
    frame.timestamp_us = clock.elapsed().as_micros() as i64;
    source.capture_frame(&frame);
    true
}

#[cfg(test)]
mod tests {
    use super::fitted;
    #[test]
    fn large_screens_fit_the_send_size_with_even_dimensions() {
        assert_eq!(fitted(1280, 720), (1280, 720));
        assert_eq!(fitted(3024, 1964), (1662, 1080));
        assert_eq!(fitted(5120, 1440), (1920, 540));
        assert_eq!(fitted(1001, 777), (1000, 776));
        assert_eq!(fitted(1, 1), (2, 2));
    }
}
