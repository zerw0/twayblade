//! One motion language for the app: fast, state-explaining transitions with a
//! single ease-out curve, and none of them under the OS Reduce Motion setting.
//! Only opacity and relative offsets animate, never layout-driving sizes, so a
//! transition cannot re-measure the timeline on every frame.
use crate::theme::scaled_px as px;
use gpui_kit::component::h_flex;
use gpui_kit::*;
use std::time::Duration;

/// Hover/press feedback and small state changes.
#[allow(dead_code)]
pub(super) const FAST: Duration = Duration::from_millis(120);
/// Panels, popovers and dialogs arriving.
pub(super) const ENTER: Duration = Duration::from_millis(200);
/// Distance an entering popover rises, in points.
const RISE: f32 = 6.;
/// Distance an entering side panel slides, in points.
const SLIDE: f32 = 16.;

/// Exponential-style ease-out: quick start, gentle stop, no overshoot.
pub(super) fn ease_out(t: f32) -> f32 {
    1. - (1. - t.clamp(0., 1.)).powi(4)
}

/// UI tests do not advance time, so they see every transition's final state,
/// exactly as under Reduce Motion.
const SETTLED: bool = cfg!(test);

fn entering(id: impl Into<ElementId>) -> Animation {
    let _ = id;
    Animation::new(ENTER).with_easing(ease_out)
}

/// Fade in and rise a few points into place (popovers, dialogs, menus).
pub(super) fn rise<E>(element: E, id: impl Into<ElementId> + Clone, reduce: bool) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if reduce || SETTLED {
        return element.into_any_element();
    }
    element
        .with_animation(id.clone(), entering(id), |element, t| {
            element.opacity(t).relative().top(px((1. - t) * RISE))
        })
        .into_any_element()
}

/// Fade in while sliding from the trailing edge (Info, thread, profile panels).
pub(super) fn slide_in<E>(element: E, id: impl Into<ElementId> + Clone, reduce: bool) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if reduce || SETTLED {
        return element.into_any_element();
    }
    element
        .with_animation(id.clone(), entering(id), |element, t| {
            element.opacity(t).relative().left(px((1. - t) * SLIDE))
        })
        .into_any_element()
}

/// A short crossfade for content that replaces itself (switching rooms).
pub(super) fn fade_in<E>(element: E, id: impl Into<ElementId> + Clone, reduce: bool) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if reduce || SETTLED {
        return element.into_any_element();
    }
    element
        .with_animation(
            id.clone(),
            Animation::new(Duration::from_millis(160)).with_easing(ease_out),
            |element, t| element.opacity(0.35 + 0.65 * t),
        )
        .into_any_element()
}

/// Three dots that pulse in sequence while someone types; static under
/// Reduce Motion. Throttled to 30 fps since it runs for as long as they type.
pub(super) fn typing_dots(color: Hsla, reduce: bool) -> AnyElement {
    let dot = |index: usize| {
        let base = div().size(px(5.)).rounded_full().bg(color);
        if reduce || SETTLED {
            return base.opacity(0.6).into_any_element();
        }
        base.with_animation(
            ("typing-dot", index),
            Animation::new(Duration::from_millis(1200))
                .repeat_synced()
                .with_max_fps(30.),
            move |dot, t| {
                // Each dot peaks a fifth of a cycle after the previous one.
                let phase = (t + 1. - index as f32 * 0.2).fract();
                let pulse = if phase < 0.5 {
                    ease_out(phase * 2.)
                } else {
                    ease_out((1. - phase) * 2.)
                };
                dot.opacity(0.3 + 0.7 * pulse)
            },
        )
        .into_any_element()
    };
    h_flex()
        .gap(px(3.))
        .px(px(6.))
        .h(px(14.))
        .rounded(px(7.))
        .children((0..3).map(dot))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::ease_out;
    #[test]
    fn ease_out_is_monotonic_and_bounded() {
        let mut last = 0.;
        for step in 0..=20 {
            let value = ease_out(step as f32 / 20.);
            assert!((0. ..=1.).contains(&value) && value >= last);
            last = value;
        }
        assert_eq!(ease_out(1.), 1.);
    }
}
