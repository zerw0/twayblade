# Voice messages (#184)

## Built

- Composer ＋ › **Voice Message** asks for the microphone (System Settings
  wording when it is denied), then the composer becomes a recording bar:
  Discard, a red dot, elapsed time, the input level, and Send. Recording
  stops by itself at 10 minutes; under half a second is not sent.
- Sent as an encrypted-capable `m.audio` attachment through the send queue:
  Ogg Opus (48 kHz mono), marked as a voice message (MSC3245) with its
  duration and a 100-point waveform, as Element sends them.
- Incoming voice messages show a play/pause button, a waveform that fills
  as it plays, and the length (elapsed while playing). The audio is
  downloaded and decrypted in memory, decoded once, and played on the
  default output; one message plays at a time; pausing resumes in place.
- Media crate (`twayblade_media::voice`): `cpal` for microphone and speaker
  on macOS, Linux (ALSA) and Windows; Opus through the libopus already in
  libwebrtc (a second copy would clash at link time); the pure-Rust `ogg`
  crate for the container; linear resampling to and from the device rate.
- Core: `MessageKind::Voice` with `MessageRow::voice` (duration, waveform);
  `Conversation::send_voice`, `Conversation::voice_audio`.
- CI: Linux jobs install `libasound2-dev` for ALSA.
- Preview: `--preview-voice`.

## Verified

- Compile: strict workspace Clippy on macOS. Offline tests: Ogg Opus round
  trip (length and signal), waveform normalisation, resampling, waveform
  bars, and a UI test for the voice bubble.
- Local Synapse (default `test` profile): a generated two-second clip is
  sent into the encrypted room; Bob sees a voice message of 2000 ms with a
  waveform, downloads it and decodes the same length.
- Offline capture of the voice bubble reviewed.

## Not verified

- Recording and playback with a real microphone and speakers on a Mac
  (not run here, to avoid a permission prompt during unattended work);
  Linux and Windows builds and audio (CI compiles them); Element playing
  our clips and us playing Element's.
