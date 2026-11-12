//! The incoming-call ringtone: a soft two-note chime generated here, so no
//! audio asset ships with the app. It loops until the handle is dropped.
//! macOS plays it through NSSound; Linux and Windows have no in-app ringtone
//! yet and rely on the notification sound.

const RATE: u32 = 44_100;
/// One cycle: two chimes, then quiet, before it repeats.
const CYCLE_SECONDS: f32 = 3.0;

/// Plays while held; dropping it stops the sound.
pub struct Ringtone {
    #[cfg(target_os = "macos")]
    sound: objc2::rc::Retained<objc2_app_kit::NSSound>,
}

/// Starts the looping ringtone, or None where playback isn't available.
pub fn start() -> Option<Ringtone> {
    #[cfg(target_os = "macos")]
    {
        use objc2::AnyThread;
        use objc2_app_kit::NSSound;
        use objc2_foundation::NSData;
        let data = NSData::with_bytes(&wav());
        let sound = NSSound::initWithData(NSSound::alloc(), &data)?;
        sound.setLoops(true);
        sound.play().then_some(Ringtone { sound })
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

impl Drop for Ringtone {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        self.sound.stop();
    }
}

/// 16-bit mono PCM WAV of one ring cycle.
pub fn wav() -> Vec<u8> {
    // (start s, frequency Hz): E5 then B5, twice.
    const NOTES: [(f32, f32); 4] = [(0.0, 659.25), (0.16, 987.77), (0.9, 659.25), (1.06, 987.77)];
    let samples = (RATE as f32 * CYCLE_SECONDS) as usize;
    let mut pcm = vec![0f32; samples];
    for (start, frequency) in NOTES {
        let first = (start * RATE as f32) as usize;
        for (i, sample) in pcm[first..].iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            if t > 1.2 {
                break;
            }
            let attack = (t / 0.006).min(1.0);
            let decay = (-t / 0.22).exp();
            let phase = std::f32::consts::TAU * frequency * t;
            *sample += attack * decay * (phase.sin() + 0.25 * (2.0 * phase).sin());
        }
    }
    let mut out = Vec::with_capacity(44 + samples * 2);
    let data_len = (samples * 2) as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in pcm {
        let value = (sample * 0.28).clamp(-1.0, 1.0) * i16::MAX as f32;
        out.extend_from_slice(&(value as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn ringtone_is_a_valid_unclipped_audible_wav() {
        let wav = super::wav();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let data = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(wav.len(), 44 + data);
        let samples: Vec<i16> = wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect();
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 3000, "audible");
        assert!(peak < i16::MAX as u16, "never clips");
        // The cycle ends quiet so the loop has no click.
        assert!(samples[samples.len() - 100..].iter().all(|s| s.abs() < 50));
    }

    /// Physical check: plays one cycle through the output device.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "plays sound on this machine"]
    fn ringtone_plays() {
        let ring = super::start().expect("NSSound accepts the generated WAV");
        std::thread::sleep(std::time::Duration::from_secs(3));
        drop(ring);
    }
}
