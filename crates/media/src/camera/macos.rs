use super::*;
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
};
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_av_foundation::*;
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::*;
use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};
use std::{sync::mpsc, time::Instant};

pub fn devices() -> Vec<Device> {
    // SAFETY: only immutable device descriptions leave the current thread.
    unsafe {
        let Some(media) = AVMediaTypeVideo else {
            return Vec::new();
        };
        #[allow(deprecated)]
        AVCaptureDevice::devicesWithMediaType(media)
            .iter()
            .map(|d| Device {
                id: d.uniqueID().to_string(),
                name: d.localizedName().to_string(),
            })
            .collect()
    }
}
struct Ivars {
    source: NativeVideoSource,
    clock: Instant,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "TwaybladeNativeCameraSink"]
    #[ivars=Ivars]
    struct Sink;
    unsafe impl NSObjectProtocol for Sink {}
    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for Sink {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn capture(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            // SAFETY: AVFoundation owns the sample for this callback. Lock the
            // BGRA buffer read-only and copy it before unlocking; no pointer or
            // CVPixelBuffer is retained across this callback.
            unsafe {
                self.frame(sample);
            }
        }
    }
);
impl Sink {
    unsafe fn frame(&self, sample: &CMSampleBuffer) {
        let Some(pixel) = (unsafe { sample.image_buffer() }) else {
            return;
        };
        let width = CVPixelBufferGetWidth(&pixel);
        let height = CVPixelBufferGetHeight(&pixel);
        let stride = CVPixelBufferGetBytesPerRow(&pixel);
        if width == 0
            || height == 0
            || width > 1920
            || height > 1080
            || stride < width * 4
            || stride > 16_384
        {
            return;
        }
        // SAFETY: lock success guarantees a valid BGRA base address for the
        // platform-reported stride and height. All sizes are capped above.
        unsafe {
            if CVPixelBufferLockBaseAddress(&pixel, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return;
            }
            let ptr = CVPixelBufferGetBaseAddress(&pixel);
            if !ptr.is_null() {
                let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), stride * height);
                let mut buffer = I420Buffer::new(width as u32, height as u32);
                let (sy, su, sv) = buffer.strides();
                let (y, u, v) = buffer.data_mut();
                yuv_helper::argb_to_i420(
                    bytes,
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
                let mut frame = VideoFrame::new(VideoRotation::VideoRotation0, buffer);
                frame.timestamp_us = self.ivars().clock.elapsed().as_micros() as i64;
                self.ivars().source.capture_frame(&frame);
            }
            CVPixelBufferUnlockBaseAddress(&pixel, CVPixelBufferLockFlags::ReadOnly);
        }
    }
}

pub struct Camera {
    source: NativeVideoSource,
    stop: Option<mpsc::SyncSender<()>>,
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
            let (stop_tx, stop_rx) = mpsc::sync_channel(1);
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("twayblade-camera".into())
                .spawn(move || {
                    let result = unsafe { open(&id, feed) };
                    match result {
                        Ok((session, output, sink, queue)) => {
                            if ready_tx.send(Ok(())).is_err() {
                                unsafe {
                                    session.stopRunning();
                                }
                                return;
                            }
                            let _ = stop_rx.recv();
                            unsafe {
                                session.stopRunning();
                                output.setSampleBufferDelegate_queue(None, None);
                            }
                            // Drain in-flight callbacks before releasing the delegate.
                            queue.exec_sync(|| {});
                            drop(sink);
                        }
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                        }
                    }
                })
                .map_err(|_| MediaError::DeviceUnavailable)?;
            ready_rx
                .recv_timeout(std::time::Duration::from_secs(15))
                .map_err(|_| MediaError::DeviceUnavailable)??;
            Ok(Self {
                source,
                stop: Some(stop_tx),
            })
        })
        .await
        .map_err(|_| MediaError::DeviceUnavailable)?
    }
}
type CaptureObjects = (
    Retained<AVCaptureSession>,
    Retained<AVCaptureVideoDataOutput>,
    Retained<Sink>,
    dispatch2::DispatchRetained<dispatch2::DispatchQueue>,
);
unsafe fn open(id: &str, source: NativeVideoSource) -> Result<CaptureObjects, MediaError> {
    // SAFETY: every Objective-C object stays on this worker. Device is resolved
    // from its stable AVFoundation unique ID. Session topology is checked.
    unsafe {
        let device = AVCaptureDevice::deviceWithUniqueID(&NSString::from_str(id))
            .ok_or(MediaError::DeviceUnavailable)?;
        let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&device)
            .map_err(|_| MediaError::DeviceUnavailable)?;
        let session = AVCaptureSession::new();
        if !session.canAddInput(&input) {
            return Err(MediaError::DeviceUnavailable);
        }
        session.addInput(&input);
        if session.canSetSessionPreset(AVCaptureSessionPreset1280x720) {
            session.setSessionPreset(AVCaptureSessionPreset1280x720);
        }
        let output = AVCaptureVideoDataOutput::new();
        output.setAlwaysDiscardsLateVideoFrames(true);
        let pixel = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
        let key = NSString::from_str("PixelFormatType");
        let settings: Retained<NSDictionary<NSString, objc2::runtime::AnyObject>> =
            NSDictionary::from_slices::<NSString>(&[&*key], &[pixel.as_ref()]);
        output.setVideoSettings(Some(&settings));
        if !session.canAddOutput(&output) {
            return Err(MediaError::DeviceUnavailable);
        }
        session.addOutput(&output);
        let sink = Sink::alloc().set_ivars(Ivars {
            source,
            clock: Instant::now(),
        });
        let sink: Retained<Sink> = msg_send![super(sink), init];
        let queue = dispatch2::DispatchQueue::new("com.twayblade.camera", None);
        output.setSampleBufferDelegate_queue(Some(ProtocolObject::from_ref(&*sink)), Some(&queue));
        session.startRunning();
        if !session.isRunning() {
            return Err(MediaError::DeviceUnavailable);
        }
        Ok((session, output, sink, queue))
    }
}
impl CaptureSource for Camera {
    fn source(&self) -> RtcVideoSource {
        RtcVideoSource::Native(self.source.clone())
    }
    fn stop(&mut self) {
        if let Some(send) = self.stop.take() {
            let _ = send.try_send(());
        }
    }
}
impl Drop for Camera {
    fn drop(&mut self) {
        self.stop();
    }
}
