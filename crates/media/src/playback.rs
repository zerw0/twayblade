//! Call-local receiver preferences, keyed by authenticated MatrixRTC identity.
use std::collections::{HashMap, HashSet};

/// Gain is finite and bounded, so equality is reflexive (including for snapshots).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playback {
    volume: f32,
    pub muted: bool,
}
impl Eq for Playback {}
impl Default for Playback {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}
impl Playback {
    /// Linear gain: 0 is silence, 1 is 100%, 2 is 200%.
    pub fn volume(self) -> f32 {
        self.volume
    }
    pub fn gain(self) -> f64 {
        if self.muted { 0.0 } else { self.volume as f64 }
    }
    /// Clamp finite values to 0–2. Reject NaN and infinities without mutation.
    pub fn set_volume(&mut self, volume: f32) -> Result<(), super::MediaError> {
        if !volume.is_finite() {
            return Err(super::MediaError::InvalidVolume);
        }
        self.volume = volume.clamp(0.0, 2.0);
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct PlaybackStore(HashMap<String, Playback>);
impl PlaybackStore {
    pub fn get(&self, id: &str) -> Playback {
        self.0.get(id).copied().unwrap_or_default()
    }
    pub fn set(&mut self, id: String, playback: Playback) {
        self.0.insert(id, playback);
    }
    pub fn retain(&mut self, members: &HashSet<String>) {
        self.0.retain(|id, _| members.contains(id));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_mute_preserve_the_selected_volume() {
        let mut p = Playback::default();
        p.set_volume(-1.0).unwrap();
        assert_eq!(p.volume(), 0.0);
        p.set_volume(3.0).unwrap();
        assert_eq!(p.volume(), 2.0);
        p.set_volume(0.375).unwrap();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(p.set_volume(value).is_err());
            assert_eq!(p.volume(), 0.375);
        }
        p.muted = true;
        assert_eq!(p.gain(), 0.0);
        p.muted = false;
        assert_eq!(p.gain(), 0.375);
    }
    #[test]
    fn departure_and_new_call_reset_preferences() {
        let mut store = PlaybackStore::default();
        let mut p = Playback::default();
        p.set_volume(0.5).unwrap();
        p.muted = true;
        store.set("alice:device".into(), p);
        store.set("bob:device".into(), p);
        store.retain(&HashSet::from(["bob:device".into()]));
        assert_eq!(store.get("alice:device"), Playback::default());
        assert_eq!(store.get("bob:device"), p);
        assert_eq!(
            PlaybackStore::default().get("bob:device"),
            Playback::default()
        );
        store.retain(&HashSet::new());
        assert_eq!(store.get("bob:device"), Playback::default());
    }
}
