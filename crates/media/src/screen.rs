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
    /// A small still of the source for the chooser, held in memory only.
    /// None when the source gave no frame in time or is past the budget.
    pub thumbnail: Option<Thumbnail>,
}
/// A downscaled BGRA still (GPUI's upload layout), at most
/// `THUMBNAIL_WIDTH`×`THUMBNAIL_HEIGHT`, tightly packed.
#[derive(Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub bgra: Arc<[u8]>,
}
impl std::fmt::Debug for Thumbnail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumbnail")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}
pub const THUMBNAIL_WIDTH: u32 = 192;
pub const THUMBNAIL_HEIGHT: u32 = 120;
/// Only the first sources get a still; the rest show an icon.
const MAX_THUMBNAILS: usize = 16;
/// Per-source wait for a first frame, and the budget for all of them.
const THUMBNAIL_WAIT: Duration = Duration::from_millis(400);
const THUMBNAILS_BUDGET: Duration = Duration::from_secs(3);
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
        // The list itself, then the bounded stills.
        Duration::from_secs(5) + THUMBNAILS_BUDGET + THUMBNAIL_WAIT,
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
                        thumbnail: None,
                    }
                })
                .collect();
            drop(capturer);
            if sources.is_empty() {
                return Err(ScreenError::NoSources);
            }
            let mut sources = sources;
            let started = Instant::now();
            for source in sources.iter_mut().take(MAX_THUMBNAILS) {
                if started.elapsed() >= THUMBNAILS_BUDGET {
                    break;
                }
                source.thumbnail = still(backend, source.selection);
            }
            Ok(sources)
        }),
    )
    .await
    .map_err(|_| ScreenError::QueryTimeout)?
    .map_err(|_| ScreenError::WorkerUnavailable)?
}

/// One frame of `selection` through its own short-lived capturer, downscaled.
/// A capturer can't switch sources once started, so each still gets a new one.
fn still(backend: ScreenBackend, selection: ScreenSelection) -> Option<Thumbnail> {
    let mut capturer = DesktopCapturer::new(options(backend, selection.kind).ok()?)?;
    let source = capturer
        .get_source_list()
        .into_iter()
        .find(|source| source.id() == selection.id)?;
    let slot = Arc::new(std::sync::Mutex::new(None));
    let frames = slot.clone();
    capturer.start_capture(Some(source), move |result| {
        if let Ok(frame) = result
            && let Ok(mut slot) = frames.lock()
            && slot.is_none()
            && frame.width() > 0
            && frame.height() > 0
        {
            *slot = downscale(
                frame.data(),
                frame.stride() as usize,
                frame.width() as u32,
                frame.height() as u32,
            );
        }
    });
    let started = Instant::now();
    while started.elapsed() < THUMBNAIL_WAIT {
        capturer.capture_frame();
        if let Some(thumbnail) = slot.lock().ok()?.take() {
            return Some(thumbnail);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    None
}

/// Box-averages a BGRA frame into a still that fits the thumbnail size,
/// keeping the aspect ratio. None for frames whose buffer is too short.
fn downscale(data: &[u8], stride: usize, width: u32, height: u32) -> Option<Thumbnail> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    if stride < w * 4 || data.len() < stride * (h - 1) + w * 4 {
        return None;
    }
    let scale = f64::max(
        width as f64 / THUMBNAIL_WIDTH as f64,
        height as f64 / THUMBNAIL_HEIGHT as f64,
    )
    .max(1.);
    let out_w = ((width as f64 / scale).round() as usize).max(1);
    let out_h = ((height as f64 / scale).round() as usize).max(1);
    let mut bgra = vec![0u8; out_w * out_h * 4];
    for oy in 0..out_h {
        let (y0, y1) = (
            oy * h / out_h,
            ((oy + 1) * h / out_h).max(oy * h / out_h + 1),
        );
        for ox in 0..out_w {
            let (x0, x1) = (
                ox * w / out_w,
                ((ox + 1) * w / out_w).max(ox * w / out_w + 1),
            );
            // At most 4×4 samples per pixel: enough to smooth text and edges.
            let ys = (y1 - y0).div_ceil(4).max(1);
            let xs = (x1 - x0).div_ceil(4).max(1);
            let mut sum = [0u32; 3];
            let mut n = 0u32;
            for y in (y0..y1).step_by(ys) {
                for x in (x0..x1).step_by(xs) {
                    let at = y * stride + x * 4;
                    for c in 0..3 {
                        sum[c] += data[at + c] as u32;
                    }
                    n += 1;
                }
            }
            let at = (oy * out_w + ox) * 4;
            for c in 0..3 {
                bgra[at + c] = (sum[c] / n) as u8;
            }
            // Desktop frames leave alpha undefined; stills are opaque.
            bgra[at + 3] = 255;
        }
    }
    Some(Thumbnail {
        width: out_w as u32,
        height: out_h as u32,
        bgra: bgra.into(),
    })
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
            thumbnail: downscale(&[7; 64], 16, 4, 4),
        };
        let debug = format!("{source:?}");
        assert!(!debug.contains("private window title"));
        assert!(!debug.contains("7, 7"));
    }
    #[test]
    fn stills_fit_the_thumbnail_keep_aspect_and_average_colours() {
        // A 1920×1080 frame with a stride wider than its rows: left half
        // blue, right half red (BGRA).
        let (w, h, stride) = (1920usize, 1080usize, 1920 * 4 + 64);
        let mut data = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                let at = y * stride + x * 4;
                let px = if x < w / 2 {
                    [255, 0, 0, 0]
                } else {
                    [0, 0, 255, 0]
                };
                data[at..at + 4].copy_from_slice(&px);
            }
        }
        let still = downscale(&data, stride, w as u32, h as u32).unwrap();
        assert_eq!((still.width, still.height), (192, 108));
        assert_eq!(still.bgra.len(), 192 * 108 * 4);
        assert_eq!(&still.bgra[..4], &[255, 0, 0, 255]);
        let last = still.bgra.len() - 4;
        assert_eq!(&still.bgra[last..], &[0, 0, 255, 255]);
        // Tall windows fit by height; small sources are not enlarged.
        let tall = downscale(&vec![0; 400 * 4 * 1000], 400 * 4, 400, 1000).unwrap();
        assert_eq!((tall.width, tall.height), (48, 120));
        let small = downscale(&[0; 64 * 4 * 40], 64 * 4, 64, 40).unwrap();
        assert_eq!((small.width, small.height), (64, 40));
    }
    #[test]
    fn stills_refuse_short_buffers_and_bad_sizes() {
        assert!(downscale(&[0; 15], 16, 4, 1).is_none());
        assert!(downscale(&[0; 64], 8, 4, 4).is_none());
        assert!(downscale(&[], 0, 0, 0).is_none());
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
