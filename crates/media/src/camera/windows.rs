//! Webcams through Media Foundation: a synchronous source-reader loop on a
//! dedicated thread, NV12 output (the reader converts from the device format).
//! Camera privacy denial surfaces as an open error and reports as unavailable.
use super::convert::{MAX_HEIGHT, MAX_WIDTH, nv12_len};
use super::*;
use ::windows::{
    Win32::{
        Media::MediaFoundation::*,
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
    },
    core::{PCWSTR, PWSTR},
};
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// COM and Media Foundation for the current thread; undone in reverse order.
struct Runtime {
    com: bool,
}
impl Runtime {
    fn new() -> Result<Self, MediaError> {
        // A thread already initialised as STA keeps its mode; Media Foundation
        // works there too, so only undo what this call did.
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        if unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.is_err() {
            if com {
                unsafe { CoUninitialize() };
            }
            return Err(MediaError::DeviceUnavailable);
        }
        Ok(Self { com })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            if self.com {
                CoUninitialize();
            }
        }
    }
}

fn video_attributes() -> Result<IMFAttributes, MediaError> {
    let mut attributes = None;
    unsafe {
        MFCreateAttributes(&mut attributes, 2).map_err(|_| MediaError::DeviceUnavailable)?;
    }
    let attributes = attributes.ok_or(MediaError::DeviceUnavailable)?;
    unsafe {
        attributes
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(|_| MediaError::DeviceUnavailable)?;
    }
    Ok(attributes)
}
fn string(attributes: &IMFAttributes, key: &::windows::core::GUID) -> Option<String> {
    let mut value = PWSTR::null();
    let mut length = 0u32;
    unsafe {
        attributes
            .GetAllocatedString(key, &mut value, &mut length)
            .ok()?;
        let text = (!value.is_null() && length <= 4096)
            .then(|| value.to_string().ok())
            .flatten();
        CoTaskMemFree(Some(value.0.cast()));
        text.filter(|s| !s.is_empty())
    }
}

pub fn devices() -> Vec<crate::Device> {
    let Ok(_runtime) = Runtime::new() else {
        return Vec::new();
    };
    let Ok(attributes) = video_attributes() else {
        return Vec::new();
    };
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    if unsafe { MFEnumDeviceSources(&attributes, &mut list, &mut count) }.is_err() || list.is_null()
    {
        return Vec::new();
    }
    let mut found = Vec::new();
    for index in 0..count as usize {
        // SAFETY: the API returned `count` initialised interface pointers.
        // Taking each one by value releases it when `activate` drops.
        let Some(activate) = (unsafe { std::ptr::read(list.add(index)) }) else {
            continue;
        };
        let attributes: &IMFAttributes = &activate;
        let Some(id) = string(
            attributes,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
        ) else {
            continue;
        };
        let name = string(attributes, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
            .unwrap_or_else(|| "Camera".to_owned());
        found.push(crate::Device { id, name });
    }
    unsafe { CoTaskMemFree(Some(list.cast())) };
    found
}
pub fn default_device() -> Option<String> {
    devices().into_iter().next().map(|d| d.id)
}

pub struct Camera {
    source: NativeVideoSource,
    stop: Arc<AtomicBool>,
}
impl Camera {
    pub async fn start(id: &str) -> Result<Self, MediaError> {
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            let source = NativeVideoSource::new(
                VideoResolution {
                    width: 1280,
                    height: 720,
                },
                false,
            );
            let feed = source.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("twayblade-camera".into())
                .spawn(move || run(&id, feed, flag, ready_tx))
                .map_err(|_| MediaError::DeviceUnavailable)?;
            ready_rx
                .recv_timeout(Duration::from_secs(15))
                .map_err(|_| MediaError::DeviceUnavailable)??;
            Ok(Self { source, stop })
        })
        .await
        .map_err(|_| MediaError::DeviceUnavailable)?
    }
}

struct Opened {
    reader: IMFSourceReader,
    width: u32,
    height: u32,
    stride: usize,
}
const STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

fn open(id: &str) -> Result<Opened, MediaError> {
    let unavailable = |_| MediaError::DeviceUnavailable;
    let attributes = video_attributes()?;
    let link: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
    unsafe {
        attributes
            .SetString(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                PCWSTR(link.as_ptr()),
            )
            .map_err(unavailable)?;
        let media_source = MFCreateDeviceSource(&attributes).map_err(unavailable)?;
        let mut reader_attributes = None;
        MFCreateAttributes(&mut reader_attributes, 1).map_err(unavailable)?;
        let reader_attributes = reader_attributes.ok_or(MediaError::DeviceUnavailable)?;
        reader_attributes
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(unavailable)?;
        let reader = MFCreateSourceReaderFromMediaSource(&media_source, &reader_attributes)
            .map_err(unavailable)?;
        // Ask for 720p NV12; a device that cannot do that size gets its own.
        let request = |sized: bool| -> ::windows::core::Result<()> {
            let media_type = MFCreateMediaType()?;
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            if sized {
                media_type.SetUINT64(&MF_MT_FRAME_SIZE, (1280u64 << 32) | 720)?;
            }
            reader.SetCurrentMediaType(STREAM, None, &media_type)
        };
        request(true)
            .or_else(|_| request(false))
            .map_err(unavailable)?;
        let current = reader.GetCurrentMediaType(STREAM).map_err(unavailable)?;
        let size = current.GetUINT64(&MF_MT_FRAME_SIZE).map_err(unavailable)?;
        let (width, height) = ((size >> 32) as u32, size as u32);
        let stride = current
            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
            .map(|s| s as i32)
            .ok()
            .filter(|s| *s > 0)
            .map_or(width as usize, |s| s as usize);
        if width > MAX_WIDTH || height > MAX_HEIGHT || nv12_len(stride, width, height).is_none() {
            return Err(MediaError::DeviceUnavailable);
        }
        Ok(Opened {
            reader,
            width,
            height,
            stride,
        })
    }
}

/// Owns COM, Media Foundation and the device for the whole capture. Locals
/// drop in reverse order, so the reader goes before the runtime shuts down.
fn run(
    id: &str,
    source: NativeVideoSource,
    stop: Arc<AtomicBool>,
    ready: mpsc::SyncSender<Result<(), MediaError>>,
) {
    let runtime = Runtime::new();
    let opened = runtime.and_then(|runtime| open(id).map(|opened| (runtime, opened)));
    let (_runtime, opened) = match opened {
        Ok(pair) => pair,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    let clock = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let mut flags = 0u32;
        let mut sample = None;
        let read = unsafe {
            opened
                .reader
                .ReadSample(STREAM, 0, None, Some(&mut flags), None, Some(&mut sample))
        };
        let ended = flags & (MF_SOURCE_READERF_ERROR.0 | MF_SOURCE_READERF_ENDOFSTREAM.0) as u32;
        if read.is_err() || ended != 0 {
            break;
        }
        let Some(sample) = sample else {
            continue;
        };
        if let Some(buffer) = convert(&sample, &opened) {
            let mut frame = VideoFrame::new(VideoRotation::VideoRotation0, buffer);
            frame.timestamp_us = clock.elapsed().as_micros() as i64;
            source.capture_frame(&frame);
        }
    }
}

fn convert(sample: &IMFSample, opened: &Opened) -> Option<I420Buffer> {
    let (width, height, stride) = (opened.width, opened.height, opened.stride);
    let needed = nv12_len(stride, width, height)?;
    unsafe {
        let media = sample.ConvertToContiguousBuffer().ok()?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut length = 0u32;
        media.Lock(&mut data, None, Some(&mut length)).ok()?;
        // SAFETY: Lock returned a readable region of `length` bytes that stays
        // valid until Unlock; it is copied into the I420 buffer before then.
        let buffer = (!data.is_null() && length as usize >= needed).then(|| {
            let bytes = std::slice::from_raw_parts(data, length as usize);
            let (luma, chroma) = bytes.split_at(stride * height as usize);
            let mut buffer = I420Buffer::new(width, height);
            let (sy, su, sv) = buffer.strides();
            let (y, u, v) = buffer.data_mut();
            yuv_helper::nv12_to_i420(
                luma,
                stride as u32,
                chroma,
                stride as u32,
                y,
                sy,
                u,
                su,
                v,
                sv,
                width as i32,
                height as i32,
            );
            buffer
        });
        let _ = media.Unlock();
        buffer
    }
}

impl CaptureSource for Camera {
    fn source(&self) -> RtcVideoSource {
        RtcVideoSource::Native(self.source.clone())
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Drop for Camera {
    fn drop(&mut self) {
        self.stop();
    }
}
