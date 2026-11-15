//! Call view logic with no GPUI in it: who the "follow the speaker" mode
//! shows, and when the full-screen controls hide. Times are passed in so the
//! rules are testable.
use std::time::{Duration, Instant};

/// Someone must speak this long, without a break, before the view moves to them.
pub(crate) const ONSET: Duration = Duration::from_millis(1000);
/// The view stays on a speaker at least this long before it can move again.
pub(crate) const HOLD: Duration = Duration::from_secs(4);
/// Full-screen controls hide after the pointer has been still this long.
pub(crate) const CONTROLS_IDLE: Duration = Duration::from_secs(3);

/// What the user chose to show large in the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CallFocus {
    /// A participant, by (user, device).
    Person(String, String),
    /// A shared screen, by video track ID.
    Screen(String),
}

const GAP: f32 = 8.;

/// The largest 16:9 tile size that fits `count` tiles in the given area, with
/// the number of columns that gives it.
pub(crate) fn fit_grid(count: usize, width: f32, height: f32) -> (f32, f32) {
    let count = count.max(1);
    let best = (1..=count)
        .map(|cols| {
            let rows = count.div_ceil(cols);
            let by_width = (width - GAP * (cols as f32 - 1.)) / cols as f32;
            let by_height = (height - GAP * (rows as f32 - 1.)) / rows as f32 * 16. / 9.;
            by_width.min(by_height)
        })
        .fold(0., f32::max)
        // A lone tile shouldn't swallow a huge stage.
        .clamp(96., 560.);
    (best.floor(), (best * 9. / 16.).floor())
}

/// Debounces the raw voice-activity flags into one stable pick.
#[derive(Debug, Default)]
pub(crate) struct SpeakerFollow {
    /// The person the view is on, and when it moved there.
    picked: Option<(String, Instant)>,
    /// Who has been speaking without a break, and since when.
    candidate: Option<(String, Instant)>,
}

impl SpeakerFollow {
    pub(crate) fn picked(&self) -> Option<&str> {
        self.picked.as_ref().map(|(id, _)| id.as_str())
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Feed the current voice-activity flags of the remote participants.
    /// Returns the person to show, if any.
    pub(crate) fn update(&mut self, now: Instant, people: &[(&str, bool)]) -> Option<&str> {
        if let Some((id, _)) = &self.picked
            && !people.iter().any(|(person, _)| person == id)
        {
            self.picked = None;
        }
        // The longest-running continuous speaker is the candidate.
        let still = |id: &str| people.iter().any(|(person, on)| *person == id && *on);
        if let Some((id, _)) = &self.candidate
            && !still(id)
        {
            self.candidate = None;
        }
        if self.candidate.is_none()
            && let Some((id, _)) = people.iter().find(|(_, on)| *on)
        {
            self.candidate = Some(((*id).to_owned(), now));
        }
        if let Some((id, since)) = &self.candidate {
            let spoken = now.saturating_duration_since(*since) >= ONSET;
            let held = self
                .picked
                .as_ref()
                .is_some_and(|(_, at)| now.saturating_duration_since(*at) < HOLD);
            let same = self.picked.as_ref().is_some_and(|(pick, _)| pick == id);
            if spoken && !held && !same {
                self.picked = Some((id.clone(), now));
            }
        }
        self.picked()
    }

    /// When `update` should next run even if nothing changes.
    pub(crate) fn next_deadline(&self, now: Instant) -> Option<Instant> {
        let (_, since) = self.candidate.as_ref()?;
        let onset = *since + ONSET;
        let hold = self.picked.as_ref().map_or(onset, |(_, at)| *at + HOLD);
        let at = onset.max(hold);
        (at > now).then_some(at)
    }
}

/// Whether the floating full-screen controls are shown.
#[derive(Debug, Default)]
pub(crate) struct ControlsVisibility {
    last_activity: Option<Instant>,
}

impl ControlsVisibility {
    /// Records pointer or key activity. True when the controls were hidden or
    /// the hide timer should be pushed back (not on every tiny mouse move).
    pub(crate) fn activity(&mut self, now: Instant, force: bool) -> bool {
        let renew = force
            || self
                .last_activity
                .is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_millis(250));
        if renew {
            self.last_activity = Some(now);
        }
        renew
    }

    /// `pinned`: the pointer is over the bar, a control has keyboard focus or
    /// a menu is open. Before any pointer movement the controls are shown.
    pub(crate) fn visible(&self, now: Instant, pinned: bool) -> bool {
        pinned
            || self
                .last_activity
                .is_none_or(|at| now.saturating_duration_since(at) < CONTROLS_IDLE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    #[test]
    fn grid_fits_the_area_and_stays_16_by_9() {
        // Two people side by side in a wide stage.
        let (w, h) = fit_grid(2, 1000., 500.);
        assert_eq!((w, h), (496., 279.));
        // Four fill a 2x2 block when the stage is tall enough.
        let (w, _) = fit_grid(4, 900., 700.);
        assert!(w <= (900. - GAP) / 2.);
        // Everything fits inside the area.
        for count in 1..=9 {
            let (w, h) = fit_grid(count, 1200., 600.);
            let cols = (1..=count)
                .find(|c| {
                    let rows = count.div_ceil(*c);
                    (*c as f32) * w + (*c as f32 - 1.) * GAP <= 1200.
                        && rows as f32 * h + (rows as f32 - 1.) * GAP <= 600.
                })
                .is_some();
            assert!(cols, "{count} tiles of {w}x{h} don't fit");
        }
    }

    #[test]
    fn a_short_burst_does_not_switch() {
        let t = Instant::now();
        let mut follow = SpeakerFollow::default();
        assert_eq!(follow.update(t, &[("a", true), ("b", false)]), None);
        assert_eq!(follow.update(ms(t, 600), &[("a", true)]), None);
        assert_eq!(follow.update(ms(t, 700), &[("a", false)]), None);
        // The break restarts the clock.
        assert_eq!(follow.update(ms(t, 800), &[("a", true)]), None);
        assert_eq!(follow.update(ms(t, 1700), &[("a", true)]), None);
        assert_eq!(follow.update(ms(t, 1800), &[("a", true)]), Some("a"));
    }

    #[test]
    fn the_pick_holds_before_moving_on() {
        let t = Instant::now();
        let mut follow = SpeakerFollow::default();
        follow.update(t, &[("a", true), ("b", false)]);
        assert_eq!(
            follow.update(ms(t, 1000), &[("a", true), ("b", false)]),
            Some("a")
        );
        // b talks over a's silence but must wait out the hold.
        follow.update(ms(t, 1500), &[("a", false), ("b", true)]);
        assert_eq!(
            follow.update(ms(t, 3000), &[("a", false), ("b", true)]),
            Some("a")
        );
        assert_eq!(
            follow.update(ms(t, 4900), &[("a", false), ("b", true)]),
            Some("a")
        );
        // The hold ends 4 s after the move at t+1000.
        assert_eq!(
            follow.update(ms(t, 5000), &[("a", false), ("b", true)]),
            Some("b")
        );
    }

    #[test]
    fn silence_keeps_the_pick_and_departure_clears_it() {
        let t = Instant::now();
        let mut follow = SpeakerFollow::default();
        follow.update(t, &[("a", true)]);
        follow.update(ms(t, 1000), &[("a", true)]);
        assert_eq!(follow.update(ms(t, 9000), &[("a", false)]), Some("a"));
        assert_eq!(follow.update(ms(t, 9500), &[("b", false)]), None);
    }

    #[test]
    fn deadline_points_at_the_next_possible_switch() {
        let t = Instant::now();
        let mut follow = SpeakerFollow::default();
        follow.update(t, &[("a", true)]);
        assert_eq!(follow.next_deadline(t), Some(ms(t, 1000)));
        follow.update(ms(t, 1000), &[("a", true)]);
        // Nothing pending once the candidate is the pick.
        follow.update(ms(t, 1100), &[("a", true), ("b", true)]);
        assert_eq!(follow.picked(), Some("a"));
        assert_eq!(follow.next_deadline(ms(t, 6000)), None);
    }

    #[test]
    fn controls_hide_after_idle_unless_pinned() {
        let t = Instant::now();
        let mut controls = ControlsVisibility::default();
        assert!(controls.visible(t, false));
        controls.activity(t, false);
        assert!(controls.visible(ms(t, 2900), false));
        assert!(!controls.visible(ms(t, 3000), false));
        assert!(controls.visible(ms(t, 9000), true));
    }
}
