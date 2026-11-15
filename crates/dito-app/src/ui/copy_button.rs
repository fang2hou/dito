//! The copy button and its confirmation: `design:motion`, board 12.02, Copy button.

use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::{CircleAppearance, CircleSize, circle_shell};
use super::callbacks::CopyCb;
use super::hover::hover_fill;
use super::motion::Motion;
use super::primitives::{icon, mix_rgb};
use super::scope::{self, ScopeHandle};
use super::tween::ease;
use crate::assets::Ic;
use crate::theme::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyPhase {
    Invert,
    Rise,
    Rest,
    Fade,
    Hold,
    Return,
    Done,
}

/// Each phase with the millisecond it ends at.
fn phases(motion: Motion) -> &'static [(CopyPhase, f32)] {
    if motion.is_reduced() {
        &[(CopyPhase::Invert, 80.), (CopyPhase::Hold, 880.), (CopyPhase::Return, 960.)]
    } else {
        &[
            (CopyPhase::Invert, 120.),
            (CopyPhase::Rise, 240.),
            (CopyPhase::Rest, 300.),
            (CopyPhase::Fade, 500.),
            (CopyPhase::Hold, 800.),
            (CopyPhase::Return, 920.),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CopyFrame {
    pub(super) phase: CopyPhase,
    /// Position inside the phase, 0 to 1.
    pub(super) progress: f32,
    /// 0 is the resting copy button, 1 the check on `primary`.
    pub(super) invert: f32,
    pub(super) halo: f32,
}

impl CopyFrame {
    const REST: Self = Self { phase: CopyPhase::Done, progress: 1., invert: 0., halo: 0. };
}

/// `from` is how inverted the button already was when the copy succeeded. The sequence skips that share of the first
/// fade, so a click on the inverted check starts at the halo and never shows the copy glyph.
pub(super) fn copy_frame(elapsed: Duration, from: f32, motion: Motion) -> CopyFrame {
    let phases = phases(motion);
    let from = from.clamp(0., 1.);
    let at = (elapsed.as_secs_f64() * 1000.) as f32 + phases[0].1 * from;
    let mut start = 0.;
    for &(phase, end) in phases {
        if at < end {
            let progress = (at - start) / (end - start);
            let (invert, halo) = match phase {
                CopyPhase::Invert => (from + (1. - from) * ease((progress - from) / (1. - from)), 0.),
                CopyPhase::Rise => (1., ease(progress)),
                CopyPhase::Rest => (1., 1.),
                CopyPhase::Fade => (1., 1. - ease(progress)),
                CopyPhase::Hold => (1., 0.),
                CopyPhase::Return => (1. - ease(progress), 0.),
                CopyPhase::Done => (0., 0.),
            };
            return CopyFrame { phase, progress, invert, halo };
        }
        start = end;
    }
    CopyFrame::REST
}

fn run_length(from: f32, motion: Motion) -> Duration {
    let phases = phases(motion);
    let ms = phases[phases.len() - 1].1 - phases[0].1 * from.clamp(0., 1.);
    Duration::from_secs_f32(ms / 1000.)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CopyRun {
    start: Instant,
    from: f32,
    /// Counts the runs of one button, so every run gets its own animation.
    key: usize,
}

impl CopyRun {
    pub(super) fn begin(previous: Option<Self>, now: Instant, motion: Motion) -> Self {
        let (from, key) = previous.map_or((0., 0), |run| (run.frame(now, motion).invert, run.key));
        Self { start: now, from, key: key + 1 }
    }

    pub(super) fn frame(&self, now: Instant, motion: Motion) -> CopyFrame {
        copy_frame(now.saturating_duration_since(self.start), self.from, motion)
    }
}

#[derive(Clone)]
struct Look {
    id: ElementId,
    /// The resting face: a button stands on its own surface, never on whatever is behind it.
    surface: Hsla,
    primary: Hsla,
    on_primary: Hsla,
    text: Hsla,
    border: Hsla,
    hover: Hsla,
    glyph: f32,
}

impl Look {
    /// The circle's face is one shape: its border, fill and halo are drawn together, so the confirmation keeps the
    /// edge of the resting button instead of stacking a second circle over it.
    fn draw(&self, frame: CopyFrame) -> Div {
        let Self { surface, primary, on_primary, text, border, hover, glyph, .. } = *self;
        let fade = |color: Hsla, v: f32| Hsla { a: color.a * v, ..color };
        let mut face = div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(mix_rgb(border, primary, frame.invert))
            .bg(mix_rgb(surface, primary, frame.invert));
        if frame.halo > 0. {
            let glow = |opacity: f32, blur: f32, spread: f32| BoxShadow {
                color: fade(primary, opacity * frame.halo),
                offset: point(px(0.), px(0.)),
                blur_radius: px(blur),
                spread_radius: px(spread),
                inset: false,
            };
            face = face.shadow(vec![glow(0.6, 3., 0.), glow(0.4, 10., 1.)]);
        }
        let mut glyphs = div().relative().size(px(glyph)).flex_none();
        if frame.invert < 1. {
            glyphs = glyphs.child(icon(Ic::Copy, glyph, fade(text, 1. - frame.invert)).absolute().top_0().left_0());
        }
        if frame.invert > 0. {
            glyphs = glyphs.child(icon(Ic::Check, glyph, fade(on_primary, frame.invert)).absolute().top_0().left_0());
        }
        face.child(hover_fill(&self.id, None, hover)).child(glyphs)
    }
}

fn begin_run(scope: &ScopeHandle, id: &ElementId) {
    let Some(reg) = scope.registry() else { return };
    let mut reg = reg.borrow_mut();
    let run = CopyRun::begin(reg.copies.get(id).copied(), Instant::now(), Motion::system());
    reg.copies.insert(id.clone(), run);
}

pub fn copy_button(id: impl Into<ElementId>, p: &Palette, on_copy: CopyCb) -> Stateful<Div> {
    copy_button_with(id, p, CircleSize::Regular, on_copy)
}

/// `on_copy` reports whether the copy succeeded; only then does the button confirm it. Nothing is scheduled while the
/// button rests: the frames come from an animation that lives exactly as long as one confirmation.
pub fn copy_button_with(id: impl Into<ElementId>, p: &Palette, size: CircleSize, on_copy: CopyCb) -> Stateful<Div> {
    let id = id.into();
    let scope = scope::motion_scope();
    let motion = Motion::system();
    let look = Look {
        id: id.clone(),
        surface: p.input_bg,
        primary: p.primary,
        on_primary: p.on_primary,
        text: p.text,
        border: p.border_strong,
        hover: p.seg,
        glyph: size.dimensions().1,
    };
    let now = Instant::now();
    let run = scope
        .registry()
        .and_then(|reg| reg.borrow().copies.get(&id).copied())
        .filter(|run| run.frame(now, motion).phase != CopyPhase::Done);
    let layer = match run {
        None => look.draw(CopyFrame::REST).into_any_element(),
        Some(run) => look
            .clone()
            .draw(run.frame(now, motion))
            .with_animation(
                (id.clone(), SharedString::from(format!("copy-{}", run.key))),
                Animation::new(run_length(run.from, motion)),
                move |_, _| look.draw(run.frame(Instant::now(), motion)),
            )
            .into_any_element(),
    };
    let key = id.clone();
    circle_shell(id, size, CircleAppearance { background: None, foreground: p.text, border: None, hover: None })
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            if on_copy(window, cx) {
                begin_run(&scope, &key);
                window.refresh();
            }
        })
        .child(layer)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{CopyPhase, CopyRun, Motion, copy_frame};

    fn at(ms: u64, from: f32, motion: Motion) -> super::CopyFrame {
        copy_frame(Duration::from_millis(ms), from, motion)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn the_full_sequence_follows_the_motion_table() {
        let cases = [
            (0, CopyPhase::Invert, 0., 0., 0.),
            (60, CopyPhase::Invert, 0.5, 0.5, 0.),
            (120, CopyPhase::Rise, 0., 1., 0.),
            (180, CopyPhase::Rise, 0.5, 1., 0.5),
            (240, CopyPhase::Rest, 0., 1., 1.),
            (299, CopyPhase::Rest, 0.98333, 1., 1.),
            (400, CopyPhase::Fade, 0.5, 1., 0.5),
            (500, CopyPhase::Hold, 0., 1., 0.),
            (799, CopyPhase::Hold, 0.99667, 1., 0.),
            (860, CopyPhase::Return, 0.5, 0.5, 0.),
            (920, CopyPhase::Done, 1., 0., 0.),
            (5000, CopyPhase::Done, 1., 0., 0.),
        ];
        for (ms, phase, progress, invert, halo) in cases {
            let frame = at(ms, 0., Motion::Full);
            assert_eq!(frame.phase, phase, "phase at {ms} ms");
            assert!(close(frame.progress, progress), "progress at {ms} ms: {}", frame.progress);
            assert!(close(frame.invert, invert), "invert at {ms} ms: {}", frame.invert);
            assert!(close(frame.halo, halo), "halo at {ms} ms: {}", frame.halo);
        }
    }

    #[test]
    fn reduce_motion_holds_the_check_without_a_halo() {
        assert!(close(at(40, 0., Motion::Reduced).invert, 0.5));
        for ms in [80, 500, 879] {
            let frame = at(ms, 0., Motion::Reduced);
            assert_eq!((frame.phase, frame.invert), (CopyPhase::Hold, 1.), "at {ms} ms");
        }
        assert!(close(at(920, 0., Motion::Reduced).invert, 0.5));
        assert_eq!(at(960, 0., Motion::Reduced).phase, CopyPhase::Done);
        assert!((0..1200).step_by(10).all(|ms| at(ms, 0., Motion::Reduced).halo == 0.));
    }

    #[test]
    fn a_second_copy_restarts_at_the_inverted_check() {
        let first = CopyRun::begin(None, Instant::now(), Motion::Full);
        let click = first.start + Duration::from_millis(600);
        let second = CopyRun::begin(Some(first), click, Motion::Full);
        assert_ne!(second.key, first.key, "a restart is a new animation");
        let frame = second.frame(click, Motion::Full);
        assert_eq!((frame.phase, frame.invert, frame.halo), (CopyPhase::Rise, 1., 0.));
        for ms in (0..800).step_by(20) {
            let frame = second.frame(click + Duration::from_millis(ms), Motion::Full);
            if frame.phase != CopyPhase::Return && frame.phase != CopyPhase::Done {
                assert_eq!(frame.invert, 1., "the copy glyph showed {ms} ms after the second copy");
            }
        }
        assert_eq!(second.frame(click + Duration::from_millis(800), Motion::Full).phase, CopyPhase::Done);

        let reduced = CopyRun::begin(Some(first), click, Motion::Reduced);
        assert_eq!(reduced.frame(click, Motion::Reduced).phase, CopyPhase::Hold);
        assert_eq!(reduced.frame(click + Duration::from_millis(880), Motion::Reduced).phase, CopyPhase::Done);
    }

    #[test]
    fn a_copy_during_the_return_continues_from_the_shown_level() {
        let first = CopyRun::begin(None, Instant::now(), Motion::Full);
        let click = first.start + Duration::from_millis(860);
        let second = CopyRun::begin(Some(first), click, Motion::Full);
        assert!(close(second.frame(click, Motion::Full).invert, 0.5));
        let later = second.frame(click + Duration::from_millis(30), Motion::Full);
        assert!(later.phase == CopyPhase::Invert && later.invert > 0.5 && later.invert < 1.);
        assert_eq!(second.frame(click + Duration::from_millis(60), Motion::Full).phase, CopyPhase::Rise);

        let finished = CopyRun::begin(Some(first), first.start + Duration::from_secs(3), Motion::Full);
        assert_eq!(finished.from, 0., "a copy after the sequence ended starts from the copy glyph");
    }
}
