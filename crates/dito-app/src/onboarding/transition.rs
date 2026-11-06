//! The math of a step change: durations, easing, and where both pages sit while one replaces the other.

use std::time::Instant;

use crate::ui::Motion;

use super::Step;

/// How long a step change takes, and the shorter cross-fade used with Reduce Motion.
pub(super) const SLIDE_S: f32 = 0.36;
pub(super) const SLIDE_REDUCED_S: f32 = 0.16;

pub(super) fn total(motion: Motion) -> f32 {
    if motion.is_reduced() { SLIDE_REDUCED_S } else { SLIDE_S }
}

fn ease_out(u: f32) -> f32 {
    1. - (1. - u.clamp(0., 1.)).powi(3)
}

fn ease_in_out(u: f32) -> f32 {
    let u = u.clamp(0., 1.);
    u * u * (3. - 2. * u)
}

/// A step change in progress: the page of `from` slides out while the current step's page slides in. Forward moves
/// slide toward the left, going back toward the right.
pub(super) struct Slide {
    pub(super) from: Step,
    pub(super) forward: bool,
    pub(super) motion: Motion,
    pub(super) at: Instant,
}

/// Where both pages are `t` seconds into a slide: their opacities and horizontal offsets, plus the art panel's.
pub(super) struct SlideView {
    pub(super) out_a: f32,
    pub(super) in_a: f32,
    pub(super) out_dx: f32,
    pub(super) in_dx: f32,
    pub(super) art_dx: f32,
}

/// Both pages `t` seconds into a slide that moves `forward` (or back). The old page leaves toward where we are
/// going and is gone early; the new one fades in late and arrives from the other side, so the two are barely
/// visible together. Reduce Motion keeps both in place and only cross-fades. The art panel follows the page
/// slightly later.
pub(super) fn view(forward: bool, motion: Motion, t: f32) -> SlideView {
    let reduced = motion.is_reduced();
    let total = total(motion);
    let u = (t / total).clamp(0., 1.);
    let sign = if forward { -1. } else { 1. };
    let (out_a, in_a) = if reduced {
        (1. - ease_in_out(u), ease_in_out(u))
    } else {
        (1. - ease_in_out(u / 0.4), ease_in_out((u - 0.35) / 0.65))
    };
    let (out_dx, in_dx) = if reduced { (0., 0.) } else { (sign * 90. * ease_out(u), -sign * 90. * (1. - ease_out(u))) };
    let art_u = ((t - 0.04) / (total - 0.04)).clamp(0., 1.);
    let art_dx = if reduced { 0. } else { -sign * 60. * (1. - ease_out(art_u)) };
    SlideView { out_a, in_a, out_dx, in_dx, art_dx }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_motion_only_cross_fades() {
        let v = view(true, Motion::Reduced, 0.08);
        assert!(v.out_dx.abs() < 1e-6);
        assert!(v.in_dx.abs() < 1e-6);
        assert!(v.art_dx.abs() < 1e-6);
        assert!((v.out_a + v.in_a - 1.).abs() < 1e-6);
    }
}
