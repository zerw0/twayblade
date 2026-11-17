//! Voice messages (#184): microphone recording and playback through `cpal`,
//! Opus through the libopus that libwebrtc already links (a second copy
//! would clash at link time), and the Ogg container. PCM is 48 kHz mono
//! `i16` throughout; devices are resampled to and from it.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Opus sample rate and frame (20 ms).
pub const RATE: u32 = 48_000;
const FRAME: usize = 960;
/// Longest recording; recording stops by itself here.
pub const MAX_DURATION: Duration = Duration::from_secs(10 * 60);
/// Waveform points sent with a voice message (MSC3245 shows up to ~100).
pub const WAVEFORM_POINTS: usize = 100;
/// Opus encoder delay skipped on decode (typical libopus lookahead).
const PRE_SKIP: u16 = 312;

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum VoiceError {
    #[error("voice_no_device")]
    NoDevice,
    #[error("voice_device_failed")]
    Device,
    #[error("voice_codec_failed")]
    Codec,
    #[error("voice_empty")]
    Empty,
    #[error("voice_invalid_audio")]
    Invalid,
}

#[allow(
    unsafe_code,
    reason = "libopus C API from libwebrtc: encoder/decoder handles are created, \
              used and destroyed only inside the safe wrappers below"
)]
mod opus {
    use super::{FRAME, RATE, VoiceError};
    use std::ffi::c_int;

    #[repr(C)]
    struct Encoder {
        _private: [u8; 0],
    }
    #[repr(C)]
    struct Decoder {
        _private: [u8; 0],
    }
    const APPLICATION_VOIP: c_int = 2048;
    const OK: c_int = 0;
    unsafe extern "C" {
        fn opus_encoder_create(
            fs: i32,
            channels: c_int,
            application: c_int,
            error: *mut c_int,
        ) -> *mut Encoder;
        fn opus_encode(
            st: *mut Encoder,
            pcm: *const i16,
            frame_size: c_int,
            data: *mut u8,
            max_data_bytes: i32,
        ) -> i32;
        fn opus_encoder_destroy(st: *mut Encoder);
        fn opus_decoder_create(fs: i32, channels: c_int, error: *mut c_int) -> *mut Decoder;
        fn opus_decode(
            st: *mut Decoder,
            data: *const u8,
            len: i32,
            pcm: *mut i16,
            frame_size: c_int,
            decode_fec: c_int,
        ) -> c_int;
        fn opus_decoder_destroy(st: *mut Decoder);
    }

    /// Encodes 48 kHz mono PCM into 20 ms Opus packets (the last one padded).
    pub(super) fn encode(pcm: &[i16]) -> Result<Vec<Vec<u8>>, VoiceError> {
        let mut error = 0;
        // SAFETY: valid arguments; the result is checked before use.
        let encoder = unsafe { opus_encoder_create(RATE as i32, 1, APPLICATION_VOIP, &mut error) };
        if encoder.is_null() || error != OK {
            return Err(VoiceError::Codec);
        }
        let mut packets = Vec::with_capacity(pcm.len() / FRAME + 1);
        let mut frame = [0i16; FRAME];
        let mut buffer = [0u8; 1500];
        let mut result = Ok(());
        for chunk in pcm.chunks(FRAME) {
            frame.fill(0);
            frame[..chunk.len()].copy_from_slice(chunk);
            // SAFETY: `frame` holds FRAME samples and `buffer` its length.
            let written = unsafe {
                opus_encode(
                    encoder,
                    frame.as_ptr(),
                    FRAME as c_int,
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                )
            };
            if written < 0 {
                result = Err(VoiceError::Codec);
                break;
            }
            packets.push(buffer[..written as usize].to_vec());
        }
        // SAFETY: created above and not used afterwards.
        unsafe { opus_encoder_destroy(encoder) };
        result.map(|()| packets)
    }

    /// Decodes Opus packets (mono, 48 kHz). Bad packets end decoding.
    pub(super) fn decode<'a>(
        packets: impl Iterator<Item = &'a [u8]>,
        limit: usize,
    ) -> Result<Vec<i16>, VoiceError> {
        let mut error = 0;
        // SAFETY: valid arguments; the result is checked before use.
        let decoder = unsafe { opus_decoder_create(RATE as i32, 1, &mut error) };
        if decoder.is_null() || error != OK {
            return Err(VoiceError::Codec);
        }
        // Up to 120 ms per packet.
        let mut frame = [0i16; 5760];
        let mut pcm = Vec::new();
        for packet in packets {
            if packet.is_empty() || pcm.len() >= limit {
                continue;
            }
            // SAFETY: `frame` holds 5760 samples, the largest Opus frame.
            let samples = unsafe {
                opus_decode(
                    decoder,
                    packet.as_ptr(),
                    packet.len() as i32,
                    frame.as_mut_ptr(),
                    frame.len() as c_int,
                    0,
                )
            };
            if samples < 0 {
                break;
            }
            pcm.extend_from_slice(&frame[..samples as usize]);
        }
        // SAFETY: created above and not used afterwards.
        unsafe { opus_decoder_destroy(decoder) };
        pcm.truncate(limit);
        Ok(pcm)
    }
}

/// `OpusHead` for mono 48 kHz (RFC 7845).
fn opus_head() -> Vec<u8> {
    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(1);
    head.extend_from_slice(&PRE_SKIP.to_le_bytes());
    head.extend_from_slice(&RATE.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    head
}

fn opus_tags() -> Vec<u8> {
    let vendor = b"Twayblade";
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());
    tags
}

/// 48 kHz mono PCM as an Ogg Opus file.
pub fn encode_ogg(pcm: &[i16]) -> Result<Vec<u8>, VoiceError> {
    use ogg::writing::{PacketWriteEndInfo, PacketWriter};
    if pcm.is_empty() {
        return Err(VoiceError::Empty);
    }
    let packets = opus::encode(pcm)?;
    let serial = 0x5477_6179;
    let mut writer = PacketWriter::new(Vec::new());
    let io = |_| VoiceError::Codec;
    writer
        .write_packet(opus_head(), serial, PacketWriteEndInfo::EndPage, 0)
        .map_err(io)?;
    writer
        .write_packet(opus_tags(), serial, PacketWriteEndInfo::EndPage, 0)
        .map_err(io)?;
    let count = packets.len();
    for (index, packet) in packets.into_iter().enumerate() {
        let granule = u64::from(PRE_SKIP) + ((index + 1) * FRAME) as u64;
        let end = if index + 1 == count {
            PacketWriteEndInfo::EndStream
        } else if (index + 1) % 50 == 0 {
            // About one page a second.
            PacketWriteEndInfo::EndPage
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        writer
            .write_packet(packet, serial, end, granule)
            .map_err(io)?;
    }
    Ok(writer.into_inner())
}

/// An Ogg Opus file as 48 kHz mono PCM, at most `MAX_DURATION` long.
pub fn decode_ogg(bytes: &[u8]) -> Result<Vec<i16>, VoiceError> {
    let mut reader = ogg::reading::PacketReader::new(std::io::Cursor::new(bytes));
    let mut packets = Vec::new();
    let mut head = None;
    while let Some(packet) = reader.read_packet().map_err(|_| VoiceError::Invalid)? {
        if head.is_none() {
            if !packet.data.starts_with(b"OpusHead") || packet.data.len() < 19 {
                return Err(VoiceError::Invalid);
            }
            // Stereo voice messages are rare; only mono is decoded.
            if packet.data[9] != 1 {
                return Err(VoiceError::Invalid);
            }
            head = Some(u16::from_le_bytes([packet.data[10], packet.data[11]]));
            continue;
        }
        if packet.data.starts_with(b"OpusTags") {
            continue;
        }
        packets.push(packet.data);
    }
    let skip = usize::from(head.ok_or(VoiceError::Invalid)?);
    let limit = (MAX_DURATION.as_secs() as usize) * RATE as usize + skip;
    let pcm = opus::decode(packets.iter().map(Vec::as_slice), limit)?;
    Ok(pcm.get(skip..).unwrap_or_default().to_vec())
}

/// Peak-normalized loudness in `points` buckets, each 0–1.
pub fn waveform(pcm: &[i16], points: usize) -> Vec<f32> {
    if pcm.is_empty() || points == 0 {
        return Vec::new();
    }
    let size = pcm.len().div_ceil(points);
    let levels: Vec<f32> = pcm
        .chunks(size)
        .map(|chunk| {
            let sum: f64 = chunk.iter().map(|s| f64::from(*s).powi(2)).sum();
            (sum / chunk.len() as f64).sqrt() as f32
        })
        .collect();
    let peak = levels.iter().copied().fold(0f32, f32::max);
    if peak <= 0. {
        return vec![0.; levels.len()];
    }
    levels.into_iter().map(|level| level / peak).collect()
}

/// Duration of 48 kHz PCM.
pub fn duration(samples: usize) -> Duration {
    Duration::from_millis((samples as u64 * 1000) / u64::from(RATE))
}

/// Streaming linear resampler for one channel.
struct Resampler {
    step: f64,
    position: f64,
    previous: f32,
}
impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        Self {
            step: f64::from(from) / f64::from(to),
            position: 0.,
            previous: 0.,
        }
    }
    /// Feeds `input` and appends resampled output.
    fn push(&mut self, input: &[f32], output: &mut Vec<f32>) {
        for &sample in input {
            while self.position <= 1. {
                let t = self.position as f32;
                output.push(self.previous + (sample - self.previous) * t);
                self.position += self.step;
            }
            self.position -= 1.;
            self.previous = sample;
        }
    }
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1., 1.) * f32::from(i16::MAX)) as i16
}

/// A microphone recording in progress. Dropping it cancels.
pub struct Recorder {
    stop: Arc<AtomicBool>,
    pcm: Arc<Mutex<Vec<i16>>>,
    /// Latest input level, 0–1000.
    level: Arc<AtomicU32>,
    thread: Option<std::thread::JoinHandle<()>>,
    failed: Arc<AtomicBool>,
}

/// A finished recording, ready to send.
pub struct Recording {
    pub ogg: Vec<u8>,
    pub duration: Duration,
    pub waveform: Vec<f32>,
}

impl Recorder {
    /// Starts recording from the default input device. The caller checks the
    /// microphone permission first.
    pub fn start() -> Result<Self, VoiceError> {
        let stop = Arc::new(AtomicBool::new(false));
        let pcm = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let thread = {
            let (stop, pcm, level, failed) =
                (stop.clone(), pcm.clone(), level.clone(), failed.clone());
            std::thread::Builder::new()
                .name("voice-recorder".into())
                .spawn(move || {
                    // The stream lives on this thread; some hosts' streams
                    // cannot move between threads.
                    let stream = match input_stream(pcm, level, failed, stop.clone()) {
                        Ok(stream) => stream,
                        Err(error) => {
                            let _ = ready.send(Err(error));
                            return;
                        }
                    };
                    let _ = ready.send(Ok(()));
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    drop(stream);
                })
                .map_err(|_| VoiceError::Device)?
        };
        started
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| VoiceError::Device)??;
        Ok(Self {
            stop,
            pcm,
            level,
            thread: Some(thread),
            failed,
        })
    }

    pub fn elapsed(&self) -> Duration {
        duration(self.pcm.lock().map(|pcm| pcm.len()).unwrap_or(0))
    }

    /// Current loudness, 0–1.
    pub fn level(&self) -> f32 {
        self.level.load(Ordering::Relaxed) as f32 / 1000.
    }

    /// The device failed or the length limit was reached.
    pub fn stopped(&self) -> bool {
        self.failed.load(Ordering::Relaxed) || self.elapsed() >= MAX_DURATION
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// Stops and encodes. Silence-only or very short recordings still encode;
    /// an empty one is `Empty`.
    pub fn finish(mut self) -> Result<Recording, VoiceError> {
        self.halt();
        let pcm = std::mem::take(&mut *self.pcm.lock().map_err(|_| VoiceError::Device)?);
        if pcm.len() < RATE as usize / 2 {
            return Err(VoiceError::Empty);
        }
        Ok(Recording {
            ogg: encode_ogg(&pcm)?,
            duration: duration(pcm.len()),
            waveform: waveform(&pcm, WAVEFORM_POINTS),
        })
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.halt();
    }
}

fn input_stream(
    pcm: Arc<Mutex<Vec<i16>>>,
    level: Arc<AtomicU32>,
    failed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) -> Result<cpal::Stream, VoiceError> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or(VoiceError::NoDevice)?;
    let supported = device
        .default_input_config()
        .map_err(|_| VoiceError::Device)?;
    let channels = usize::from(supported.channels()).max(1);
    let mut resampler = Resampler::new(supported.sample_rate(), RATE);
    let limit = MAX_DURATION.as_secs() as usize * RATE as usize;
    let mut mono = Vec::new();
    let mut resampled = Vec::new();
    let mut take = move |samples: &mut dyn Iterator<Item = f32>| {
        mono.clear();
        let all: Vec<f32> = samples.collect();
        mono.extend(
            all.chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32),
        );
        let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();
        level.store((rms.min(1.) * 1000.) as u32, Ordering::Relaxed);
        resampled.clear();
        resampler.push(&mono, &mut resampled);
        if let Ok(mut pcm) = pcm.lock() {
            let room = limit.saturating_sub(pcm.len());
            pcm.extend(resampled.iter().take(room).map(|s| to_i16(*s)));
            if room == 0 {
                stop.store(true, Ordering::Relaxed);
            }
        }
    };
    let on_error = move |_| failed.store(true, Ordering::Relaxed);
    let config = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
            config,
            move |data, _| take(&mut data.iter().copied()),
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
            config,
            move |data, _| take(&mut data.iter().map(|s| f32::from(*s) / 32768.)),
            on_error,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream::<i32, _, _>(
            config,
            move |data, _| take(&mut data.iter().map(|s| *s as f32 / 2_147_483_648.)),
            on_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream::<u16, _, _>(
            config,
            move |data, _| take(&mut data.iter().map(|s| (f32::from(*s) - 32768.) / 32768.)),
            on_error,
            None,
        ),
        _ => return Err(VoiceError::Device),
    }
    .map_err(|_| VoiceError::Device)?;
    stream.play().map_err(|_| VoiceError::Device)?;
    Ok(stream)
}

/// Plays 48 kHz mono PCM on the default output device. Dropping it stops.
pub struct Player {
    stop: Arc<AtomicBool>,
    /// Samples of the source already played.
    position: Arc<AtomicUsize>,
    total: usize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Player {
    /// Starts playing `pcm` from `start` (a sample index).
    pub fn start(pcm: Arc<Vec<i16>>, start: usize) -> Result<Self, VoiceError> {
        let total = pcm.len();
        let stop = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicUsize::new(start.min(total)));
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let thread = {
            let (stop, position) = (stop.clone(), position.clone());
            std::thread::Builder::new()
                .name("voice-player".into())
                .spawn(move || {
                    let stream = match output_stream(pcm, position.clone()) {
                        Ok(stream) => stream,
                        Err(error) => {
                            let _ = ready.send(Err(error));
                            return;
                        }
                    };
                    let _ = ready.send(Ok(()));
                    while !stop.load(Ordering::Relaxed) && position.load(Ordering::Relaxed) < total
                    {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    // Let the device drain the last buffer.
                    std::thread::sleep(Duration::from_millis(60));
                    drop(stream);
                })
                .map_err(|_| VoiceError::Device)?
        };
        started
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| VoiceError::Device)??;
        Ok(Self {
            stop,
            position,
            total,
            thread: Some(thread),
        })
    }

    /// Samples played so far.
    pub fn position(&self) -> usize {
        self.position.load(Ordering::Relaxed).min(self.total)
    }

    pub fn finished(&self) -> bool {
        self.position() >= self.total
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn output_stream(
    pcm: Arc<Vec<i16>>,
    position: Arc<AtomicUsize>,
) -> Result<cpal::Stream, VoiceError> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(VoiceError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(|_| VoiceError::Device)?;
    let channels = usize::from(supported.channels()).max(1);
    let rate = supported.sample_rate();
    // Source samples per output frame.
    let step = f64::from(RATE) / f64::from(rate);
    let mut cursor = position.load(Ordering::Relaxed) as f64;
    let mut next = move || -> f32 {
        let index = cursor as usize;
        let sample = match (pcm.get(index), pcm.get(index + 1)) {
            (Some(a), Some(b)) => {
                let t = (cursor - index as f64) as f32;
                (f32::from(*a) + (f32::from(*b) - f32::from(*a)) * t) / 32768.
            }
            (Some(a), None) => f32::from(*a) / 32768.,
            _ => 0.,
        };
        cursor += step;
        position.store(cursor as usize, Ordering::Relaxed);
        sample
    };
    let config = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream::<f32, _, _>(
            config,
            move |data, _| {
                for frame in data.chunks_mut(channels) {
                    frame.fill(next());
                }
            },
            |_| {},
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream::<i16, _, _>(
            config,
            move |data, _| {
                for frame in data.chunks_mut(channels) {
                    frame.fill(to_i16(next()));
                }
            },
            |_| {},
            None,
        ),
        _ => return Err(VoiceError::Device),
    }
    .map_err(|_| VoiceError::Device)?;
    stream.play().map_err(|_| VoiceError::Device)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f32) -> Vec<i16> {
        (0..(seconds * RATE as f32) as usize)
            .map(|n| {
                let t = n as f32 / RATE as f32;
                to_i16((t * 440. * std::f32::consts::TAU).sin() * 0.5)
            })
            .collect()
    }

    #[test]
    fn ogg_opus_round_trips_length_and_signal() {
        let pcm = tone(1.5);
        let ogg = encode_ogg(&pcm).unwrap();
        assert!(ogg.starts_with(b"OggS"));
        let decoded = decode_ogg(&ogg).unwrap();
        // Whole 20 ms frames, minus the encoder delay.
        assert!(decoded.len().abs_diff(pcm.len()) <= FRAME);
        let energy =
            |s: &[i16]| s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / s.len() as f64;
        assert!(energy(&decoded) > energy(&pcm) * 0.5);
        assert!(matches!(decode_ogg(b"not ogg"), Err(VoiceError::Invalid)));
        assert!(matches!(encode_ogg(&[]), Err(VoiceError::Empty)));
    }

    #[test]
    fn waveform_is_normalized_to_the_loudest_point() {
        let mut pcm = vec![0i16; RATE as usize];
        pcm.extend(tone(1.));
        let points = waveform(&pcm, 10);
        assert_eq!(points.len(), 10);
        assert_eq!(points[0], 0.);
        assert!((points.iter().copied().fold(0f32, f32::max) - 1.).abs() < 1e-6);
        assert_eq!(duration(RATE as usize * 3).as_secs(), 3);
    }

    #[test]
    fn resampling_keeps_duration() {
        let mut resampler = Resampler::new(44_100, RATE);
        let mut out = Vec::new();
        resampler.push(&vec![0.25; 44_100], &mut out);
        assert!(out.len().abs_diff(RATE as usize) <= 2);
        // The first outputs ramp up from silence before the first input.
        assert!(out.iter().skip(2).all(|s| (s - 0.25).abs() < 1e-6));
    }
}
