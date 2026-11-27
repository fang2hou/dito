//! The recording indicator that follows a live meeting's name (`design:tools/recording/live-notes`): a 7 pt red dot
//! with a 1 pt halo that grows from 7 to 15 pt and fades from 28 percent over 800 ms, then rests for 1.2 s. Under
//! Reduce Motion it is a steady dot, and a paused recording is a muted dot with no halo.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::motion::Motion;
use super::palette::Palette;

const DOT: f32 = 7.;
const FRAME: f32 = 15.;
const HALO_FROM: f32 = 7.;
const HALO_ALPHA: f32 = 0.28;
const HALO_MS: f32 = 800.;
const REST_MS: f32 = 1200.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotState {
    Live,
    Paused,
}

/// The halo's diameter and opacity `t` of the way through one cycle, `None` while it rests.
pub fn halo_at(t: f32) -> Option<(f32, f32)> {
    let elapsed = t.clamp(0., 1.) * (HALO_MS + REST_MS);
    if elapsed >= HALO_MS {
        return None;
    }
    let progress = elapsed / HALO_MS;
    let eased = 1. - (1. - progress).powi(3);
    Some((HALO_FROM + (FRAME - HALO_FROM) * eased, HALO_ALPHA * (1. - eased)))
}

/// The indicator in a 15 pt square, so it takes the same room whether the halo is out or not.
pub fn recording_dot(id: impl Into<ElementId>, state: DotState, p: &Palette, motion: Motion) -> AnyElement {
    let id = id.into();
    let live = state == DotState::Live;
    let color = if live { p.danger } else { p.muted };
    let frame = div().relative().size(px(FRAME)).flex_none().flex().items_center().justify_center();
    let dot = div().size(px(DOT)).flex_none().rounded_full().bg(color);
    if !live || motion.is_reduced() {
        return frame.child(dot).into_any_element();
    }
    let danger = p.danger;
    let halo = move |t: f32| match halo_at(t) {
        Some((diameter, alpha)) => div()
            .absolute()
            .size(px(diameter))
            .rounded_full()
            .border_1()
            .border_color(danger.opacity(alpha))
            .into_any_element(),
        None => div().into_any_element(),
    };
    frame
        .child(div().absolute().size(px(FRAME)).flex().items_center().justify_center().child(
            div().size(px(FRAME)).flex().items_center().justify_center().with_animation(
                (id, "halo"),
                Animation::new(Duration::from_millis((HALO_MS + REST_MS) as u64)).repeat(),
                move |_, t| div().size(px(FRAME)).flex().items_center().justify_center().child(halo(t)),
            ),
        ))
        .child(dot)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::halo_at;

    #[test]
    fn the_halo_grows_from_the_dot_to_15_pt_while_it_fades_then_rests() {
        let (start, alpha) = halo_at(0.).unwrap();
        assert_eq!((start, alpha), (7., 0.28));
        let (mid, faded) = halo_at(0.2).unwrap();
        assert!(mid > 7. && mid < 15. && faded < 0.28 && faded > 0.);
        assert!(halo_at(0.399).unwrap().0 > 14.9, "the growth ends with the 800 ms");
        assert_eq!(halo_at(0.4), None, "it rests for the remaining 1.2 s");
        assert_eq!(halo_at(0.99), None);
    }
}
