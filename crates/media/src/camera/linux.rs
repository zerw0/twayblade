//! Webcams through Video4Linux2: mmap streaming of YUYV or NV12 frames.
//! V4L2 has no permission prompt; a device the user cannot open reports as
//! unavailable. Cameras that only offer compressed formats (MJPEG) are not
//! supported yet.
use super::convert::{Mode, PixelFormat, choose_mode, nv12_len, yuyv_to_i420};
use super::*;
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
use v4l::{
    Device, Format, FourCC, buffer::Type, capability::Flags, context::enum_devices,
    framesize::FrameSizeEnum, io::traits::CaptureStream, video::Capture,
};

const YUYV: FourCC = FourCC { repr: *b"YUYV" };
const NV12: FourCC = FourCC { repr: *b"NV12" };

fn is_capture_node(device: &Device) -> bool {
    device
        .query_caps()
        .is_ok_and(|caps| caps.capabilities.contains(Flags::VIDEO_CAPTURE))
}

pub fn devices() -> Vec<crate::Device> {
    let mut found = Vec::new();
    for node in enum_devices() {
        let Ok(device) = Device::with_path(node.path()) else {
            continue;
        };
        let Ok(caps) = device.query_caps() else {
            continue;
        };
        if !caps.capabilities.contains(Flags::VIDEO_CAPTURE) || supported_modes(&device).is_empty()
        {
            continue;
        }
        let Some(id) = node.path().to_str() else {
            continue;
        };
        found.push(crate::Device {
            id: id.to_owned(),
            name: if caps.card.is_empty() {
                id.to_owned()
            } else {
                caps.card
            },
        });
    }
    found
}
pub fn default_device() -> Option<String> {
    devices().into_iter().next().map(|d| d.id)
}

fn supported_modes(device: &Device) -> Vec<Mode> {
    let mut modes = Vec::new();
    let Ok(formats) = device.enum_formats() else {
        return modes;
    };
    for description in formats {
        let format = if description.fourcc == YUYV {
            PixelFormat::Yuyv
        } else if description.fourcc == NV12 {
            PixelFormat::Nv12
        } else {
            continue;
        };
        let Ok(sizes) = device.enum_framesizes(description.fourcc) else {
            continue;
        };
        for size in sizes {
            // A stepwise range is not expanded; ask for 720p when it fits.
            let candidates = match size.size {
                FrameSizeEnum::Discrete(d) => vec![(d.width, d.height)],
                FrameSizeEnum::Stepwise(s) => {
                    if (s.min_width..=s.max_width).contains(&1280)
                        && (s.min_height..=s.max_height).contains(&720)
                    {
                        vec![(1280, 720)]
                    } else {
                        Vec::new()
                    }
                }
            };
            for (width, height) in candidates {
                modes.push(Mode {
                    format,
                    width,
                    height,
                    fps: best_fps(device, description.fourcc, width, height),
                });
            }
        }
    }
    modes
}
fn best_fps(device: &Device, fourcc: FourCC, width: u32, height: u32) -> u32 {
    use v4l::frameinterval::FrameIntervalEnum;
    device
        .enum_frameintervals(fourcc, width, height)
        .map(|intervals| {
            intervals
                .into_iter()
                .filter_map(|i| match i.interval {
                    FrameIntervalEnum::Discrete(f) if f.numerator > 0 => {
                        Some(f.denominator / f.numerator)
                    }
                    _ => None,
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
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

/// Owns the device for the whole capture. The stop flag is checked between
/// frames, so the device closes within one frame interval of `stop`.
fn run(
    id: &str,
    source: NativeVideoSource,
    stop: Arc<AtomicBool>,
    ready: mpsc::SyncSender<Result<(), MediaError>>,
) {
    let opened = open(id);
    let (device, format) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let Ok(mut stream) = v4l::io::mmap::Stream::with_buffers(&device, Type::VideoCapture, 4) else {
        let _ = ready.send(Err(MediaError::DeviceUnavailable));
        return;
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    let clock = Instant::now();
    let stride = format.stride as usize;
    while !stop.load(Ordering::Acquire) {
        let Ok((data, _)) = stream.next() else {
            break;
        };
        let buffer = if format.fourcc == YUYV {
            yuyv_to_i420(data, stride, format.width, format.height)
        } else {
            nv12_to_i420(data, stride, format.width, format.height)
        };
        let Some(buffer) = buffer else {
            continue;
        };
        let mut frame = VideoFrame::new(VideoRotation::VideoRotation0, buffer);
        frame.timestamp_us = clock.elapsed().as_micros() as i64;
        source.capture_frame(&frame);
    }
}

fn nv12_to_i420(data: &[u8], stride: usize, width: u32, height: u32) -> Option<I420Buffer> {
    if data.len() < nv12_len(stride, width, height)? {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let (luma, chroma) = data.split_at(stride * h);
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
        w as i32,
        h as i32,
    );
    Some(buffer)
}

fn open(id: &str) -> Result<(Device, Format), MediaError> {
    // Only paths the enumeration could have produced.
    if !id.starts_with("/dev/video") || id.contains("..") {
        return Err(MediaError::DeviceUnavailable);
    }
    let device = Device::with_path(id).map_err(|_| MediaError::DeviceUnavailable)?;
    if !is_capture_node(&device) {
        return Err(MediaError::DeviceUnavailable);
    }
    let mode = choose_mode(&supported_modes(&device)).ok_or(MediaError::DeviceUnavailable)?;
    let fourcc = match mode.format {
        PixelFormat::Yuyv => YUYV,
        PixelFormat::Nv12 => NV12,
    };
    let actual = device
        .set_format(&Format::new(mode.width, mode.height, fourcc))
        .map_err(|_| MediaError::DeviceUnavailable)?;
    // The driver may adjust the request; accept only what can be converted.
    if actual.fourcc != YUYV && actual.fourcc != NV12 {
        return Err(MediaError::DeviceUnavailable);
    }
    Ok((device, actual))
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
