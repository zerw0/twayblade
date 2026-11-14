//! Screen sharing through libwebrtc's desktop capturer. macOS and Wayland use
//! the system picker. X11 and Windows require an explicit source selection.
//! One capture thread owns the capturer, converts
//! frames to I420 (scaled to fit `MAX_*`) and feeds a screencast source.
#[cfg(target_os = "windows")]
mod windows;
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
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

const MAX_WIDTH: u32 = 1920;
const MAX_HEIGHT: u32 = 1080;
const FRAME_INTERVAL: Duration = Duration::from_millis(66);
/// A picker left open this long without a choice ends the attempt.
pub const CHOOSE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenBackend {
    ScreenCaptureKit,
    WaylandPortal,
    X11,
    WindowsGraphicsCapture,
}
impl ScreenBackend {
    pub fn uses_system_picker(self) -> bool {
        matches!(self, Self::ScreenCaptureKit | Self::WaylandPortal)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ScreenError {
    #[error("screen_unsupported_session")]
    UnsupportedSession,
    #[error("screen_backend_unavailable_{0:?}")]
    BackendUnavailable(ScreenBackend),
    #[error("screen_selection_required")]
    SelectionRequired,
    #[error("screen_source_unavailable")]
    SourceUnavailable,
    #[error("screen_no_sources")]
    NoSources,
    #[error("screen_busy")]
    Busy,
    #[error("screen_worker_unavailable")]
    WorkerUnavailable,
    #[error("screen_start_timeout")]
    StartTimeout,
    #[error("screen_query_timeout")]
    QueryTimeout,
    #[error("screen_selection_timeout")]
    SelectionTimeout,
    #[error("screen_capture_failed")]
    CaptureFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Screen,
    Window,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenSelection {
    pub kind: SourceKind,
    pub id: u64,
}
#[derive(Clone, PartialEq, Eq)]
pub struct ScreenSource {
    pub selection: ScreenSelection,
    /// OS-provided title, never logged or persisted. None for unnamed monitors.
    pub title: Option<String>,
}
impl std::fmt::Debug for ScreenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScreenSource").finish_non_exhaustive()
    }
}
#[cfg(target_os = "macos")]
pub fn backend() -> Result<ScreenBackend, ScreenError> {
    Ok(ScreenBackend::ScreenCaptureKit)
}
#[cfg(target_os = "windows")]
pub fn backend() -> Result<ScreenBackend, ScreenError> {
    Ok(ScreenBackend::WindowsGraphicsCapture)
}
#[cfg(target_os = "linux")]
pub fn backend() -> Result<ScreenBackend, ScreenError> {
    linux_backend(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some_and(|s| !s.is_empty()),
        std::env::var_os("DISPLAY").is_some_and(|s| !s.is_empty()),
    )
}
#[cfg(any(target_os = "linux", test))]
fn linux_backend(
    session: Option<&str>,
    wayland: bool,
    x11: bool,
) -> Result<ScreenBackend, ScreenError> {
    match session {
        Some("wayland") => Ok(ScreenBackend::WaylandPortal),
        Some("x11") if x11 => Ok(ScreenBackend::X11),
        Some("x11") => Err(ScreenError::UnsupportedSession),
        Some(other) if !other.is_empty() => Err(ScreenError::UnsupportedSession),
        _ if wayland => Ok(ScreenBackend::WaylandPortal),
        _ if x11 => Ok(ScreenBackend::X11),
        _ => Err(ScreenError::UnsupportedSession),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenStatus {
    /// The first frame arrived: a screen or window was chosen.
    Live,
    /// Stopped by the caller. The SDK cannot identify picker cancellation or
    /// denial separately from a permanent capture failure.
    Ended,
    /// The SDK reports permanent failure (including an indistinguishable picker
    /// cancellation/denial), or a bounded first-frame attempt timed out.
    Failed(ScreenError),
}

pub struct ScreenCapture {
    source: NativeVideoSource,
    stop: Arc<AtomicBool>,
}
impl ScreenCapture {
    /// Starts capture and, where the platform has one, the system picker.
    /// Status changes arrive on `events`; `Ended` or `Failed` is sent last.
    pub fn start(
        events: mpsc::Sender<ScreenStatus>,
        selection: Option<ScreenSelection>,
    ) -> Result<Self, ScreenError> {
        let backend = backend()?;
        if !backend.uses_system_picker() && selection.is_none() {
            return Err(ScreenError::SelectionRequired);
        }
        static CAPTURE_SLOT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
        let permit = CAPTURE_SLOT
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
            .clone()
            .try_acquire_owned()
            .map_err(|_| ScreenError::Busy)?;
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
                let _permit = permit;
                #[cfg(target_os = "windows")]
                let _runtime = match windows::Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                // The capturer is not Send: create, drive and drop it here.
                let (mut capturer, choice) = match capturer(backend, selection) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
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
                let status = terminal_status(
                    stopped.load(Ordering::Relaxed),
                    live.load(Ordering::Relaxed),
                    clock.elapsed(),
                );
                let _ = events.try_send(status);
            })
            .map_err(|_| ScreenError::WorkerUnavailable)?;
        let ready = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| ScreenError::StartTimeout)
            .and_then(|result| result);
        if let Err(error) = ready {
            stop.store(true, Ordering::Relaxed);
            return Err(error);
        }
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
fn options(
    backend: ScreenBackend,
    kind: SourceKind,
) -> Result<DesktopCapturerOptions, ScreenError> {
    let source_type = if backend.uses_system_picker() {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            DesktopCaptureSourceType::Generic
        }
        #[cfg(target_os = "windows")]
        {
            return Err(ScreenError::UnsupportedSession);
        }
    } else {
        match kind {
            SourceKind::Screen => DesktopCaptureSourceType::Screen,
            SourceKind::Window => DesktopCaptureSourceType::Window,
        }
    };
    let mut options = DesktopCapturerOptions::new(source_type);
    options.set_include_cursor(true);
    Ok(options)
}
fn capturer(
    backend: ScreenBackend,
    selection: Option<ScreenSelection>,
) -> Result<(DesktopCapturer, Choice), ScreenError> {
    if backend.uses_system_picker() && selection.is_some() {
        return Err(ScreenError::SourceUnavailable);
    }
    let capturer = DesktopCapturer::new(options(
        backend,
        selection.map(|s| s.kind).unwrap_or(SourceKind::Screen),
    )?)
    .ok_or(ScreenError::BackendUnavailable(backend))?;
    let choice = if backend.uses_system_picker() {
        None
    } else {
        let selected = selection.ok_or(ScreenError::SelectionRequired)?;
        let sources = capturer.get_source_list();
        let index = selection_index(sources.iter().map(|source| source.id()), Some(selected.id))?;
        Some(
            sources
                .into_iter()
                .nth(index)
                .ok_or(ScreenError::SourceUnavailable)?,
        )
    };
    Ok((capturer, choice))
}
fn selection_index(
    ids: impl Iterator<Item = u64>,
    requested: Option<u64>,
) -> Result<usize, ScreenError> {
    let requested = requested.ok_or(ScreenError::SelectionRequired)?;
    let mut matches = ids.enumerate().filter(|(_, id)| *id == requested);
    let (index, _) = matches.next().ok_or(ScreenError::SourceUnavailable)?;
    if matches.next().is_some() {
        return Err(ScreenError::SourceUnavailable);
    }
    Ok(index)
}
fn terminal_status(stopped: bool, live: bool, elapsed: Duration) -> ScreenStatus {
    if stopped {
        ScreenStatus::Ended
    } else if !live && elapsed >= CHOOSE_TIMEOUT {
        ScreenStatus::Failed(ScreenError::SelectionTimeout)
    } else {
        ScreenStatus::Failed(ScreenError::CaptureFailed)
    }
}
/// X11/Windows enumeration only. System-picker platforms never enumerate or
/// fall back to a different capture backend. A timed-out native query retains
/// the single global permit until its worker returns, bounding stranded work.
pub async fn sources(kind: SourceKind) -> Result<Vec<ScreenSource>, ScreenError> {
    let backend = backend()?;
    if backend.uses_system_picker() {
        return Err(ScreenError::SelectionRequired);
    }
    static QUERY_SLOT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let permit = QUERY_SLOT
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| ScreenError::Busy)?;
    tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            #[cfg(target_os = "windows")]
            let _runtime = windows::Runtime::new()?;
            let capturer = DesktopCapturer::new(options(backend, kind)?)
                .ok_or(ScreenError::BackendUnavailable(backend))?;
            let sources: Vec<_> = capturer
                .get_source_list()
                .into_iter()
                .take(128)
                .map(|source| {
                    let title: String = source.title().chars().take(512).collect();
                    ScreenSource {
                        selection: ScreenSelection {
                            kind,
                            id: source.id(),
                        },
                        title: (!title.is_empty()).then_some(title),
                    }
                })
                .collect();
            if sources.is_empty() {
                return Err(ScreenError::NoSources);
            }
            Ok(sources)
        }),
    )
    .await
    .map_err(|_| ScreenError::QueryTimeout)?
    .map_err(|_| ScreenError::WorkerUnavailable)?
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
    use super::*;
    #[test]
    fn wayland_never_falls_back_to_x11_and_headless_sessions_fail_typed() {
        assert_eq!(
            linux_backend(Some("wayland"), true, true),
            Ok(ScreenBackend::WaylandPortal)
        );
        assert_eq!(
            linux_backend(Some("wayland"), false, true),
            Ok(ScreenBackend::WaylandPortal)
        );
        assert_eq!(
            linux_backend(Some("x11"), true, true),
            Ok(ScreenBackend::X11)
        );
        assert_eq!(
            linux_backend(None, true, true),
            Ok(ScreenBackend::WaylandPortal)
        );
        assert_eq!(linux_backend(None, false, true), Ok(ScreenBackend::X11));
        for (session, wayland, x11) in [
            (Some("tty"), false, true),
            (Some("x11"), true, false),
            (None, false, false),
        ] {
            assert_eq!(
                linux_backend(session, wayland, x11),
                Err(ScreenError::UnsupportedSession)
            );
        }
    }
    #[test]
    fn source_selection_never_uses_a_first_or_ambiguous_source() {
        assert_eq!(
            selection_index([10, 20].into_iter(), None),
            Err(ScreenError::SelectionRequired)
        );
        assert_eq!(
            selection_index([10, 20].into_iter(), Some(30)),
            Err(ScreenError::SourceUnavailable)
        );
        assert_eq!(selection_index([10, 20].into_iter(), Some(20)), Ok(1));
        assert_eq!(
            selection_index([20, 20].into_iter(), Some(20)),
            Err(ScreenError::SourceUnavailable)
        );
    }
    #[test]
    fn requested_stop_is_neutral_and_timeouts_differ_from_sdk_failures() {
        assert_eq!(
            terminal_status(true, false, CHOOSE_TIMEOUT),
            ScreenStatus::Ended
        );
        assert_eq!(
            terminal_status(false, false, CHOOSE_TIMEOUT),
            ScreenStatus::Failed(ScreenError::SelectionTimeout)
        );
        assert_eq!(
            terminal_status(false, false, Duration::ZERO),
            ScreenStatus::Failed(ScreenError::CaptureFailed)
        );
        assert_eq!(
            terminal_status(false, true, CHOOSE_TIMEOUT),
            ScreenStatus::Failed(ScreenError::CaptureFailed)
        );
    }
    #[test]
    fn capture_debug_output_never_contains_window_titles() {
        let source = ScreenSource {
            selection: ScreenSelection {
                kind: SourceKind::Window,
                id: 42,
            },
            title: Some("private window title".into()),
        };
        assert!(!format!("{source:?}").contains("private window title"));
    }
    #[test]
    fn large_screens_fit_the_send_size_with_even_dimensions() {
        assert_eq!(fitted(1280, 720), (1280, 720));
        assert_eq!(fitted(3024, 1964), (1662, 1080));
        assert_eq!(fitted(5120, 1440), (1920, 540));
        assert_eq!(fitted(1001, 777), (1000, 776));
        assert_eq!(fitted(1, 1), (2, 2));
    }
}
