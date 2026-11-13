# LiveKit WebRTC playback gain patch

Sources: crates.io libwebrtc 0.4.0 and webrtc-sys 0.3.53 (Apache-2.0;
upstream notices retained). Cargo patches keep the existing locked versions.

The only functional changes expose `RtcAudioTrack::set_playback_volume(f64)`
through CXX to WebRTC `AudioTrackInterface::SetVolume`. The bridge rejects
nonfinite values and values outside 0–2. It controls the remote source playout
gain, never publication mute or microphone gain. Twayblade applies it only to
remote tracks. No codec, encryption or transport changes.

Remove both patches when the upstream Rust SDK exposes remote playback gain.
