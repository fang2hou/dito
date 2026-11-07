//! The capsule as the HUD view draws it: one layer, or two cross-fading while the state changes.

use dito_core::controller::HudState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hud::Scale;
use crate::hud::buttons;
use crate::hud::capsule::{Capsule, Wave};
use crate::hud::timing::{COPY_FADE_S, COPY_HOLD_END, OUTCOME_FADE_S, OUTCOME_HOLD_END, OUTCOME_ICON_DELAY};
use crate::theme::Palette;
use crate::ui::Motion;

use super::{HudView, LABEL_FADE_S};

const SUCCESS_FADE_S: f32 = 0.20;
/// `hud/state`: the capsule's background, border and labels cross-fade when the dictation state changes.
pub(super) const STATE_FADE_S: f32 = 0.16;

/// The two clocks every layer of one frame reads, sampled once so cross-fading layers agree.
#[derive(Clone, Copy)]
struct FrameClock {
    elapsed: f32,
    scene: f32,
}

/// The incoming capsule sits complete underneath; the outgoing one fades out over it. Keeping the new shell
/// opaque for the whole transition means the background and border cross over without the window showing through.
fn crossfade(old: Div, next: Div, blend: f32) -> Div {
    div().relative().child(next).child(old.absolute().top_0().left_0().opacity(1. - blend))
}

impl HudView {
    pub(super) fn capsule(&self, p: Palette, cx: &mut Context<Self>) -> Stateful<Div> {
        let elapsed = self.elapsed();
        let clock = FrameClock { elapsed, scene: self.freeze.unwrap_or_else(|| self.opened.elapsed().as_secs_f32()) };
        let terminal = matches!(self.state, HudState::Inserted | HudState::Copied);
        let mut cap = self.layers(&p, clock, terminal, cx);
        if terminal && self.freeze.is_none() {
            let (hold_end, fade) = if matches!(self.state, HudState::Copied) {
                (COPY_HOLD_END, COPY_FADE_S)
            } else {
                (OUTCOME_HOLD_END, OUTCOME_FADE_S)
            };
            cap = cap.opacity((1. - (self.outcome_elapsed(elapsed) - hold_end) / fade).clamp(0., 1.));
        }
        cap.id("hud-capsule")
    }

    fn layers(&self, p: &Palette, clock: FrameClock, terminal: bool, cx: &mut Context<Self>) -> Div {
        let elapsed = clock.elapsed;
        if self.state == HudState::Recognizing
            && elapsed < LABEL_FADE_S
            && let Some(listening @ HudState::Listening { .. }) = &self.previous
        {
            let old = self
                .layer(p, clock, listening.clone(), None)
                .element(&mut |b| buttons::static_button(b, Scale::HUD, p).into_any_element());
            let next = self.interactive_layer(p, clock, self.state.clone(), self.progress(), cx);
            return crossfade(old, next, (elapsed / LABEL_FADE_S).clamp(0., 1.));
        }
        let processing_before =
            self.previous.as_ref().filter(|s| matches!(s, HudState::Recognizing | HudState::Polishing));
        let changing_phase = self.state == HudState::Polishing && self.processing.is_some();
        let catch_up = self.catch_up();
        let transition = if terminal { SUCCESS_FADE_S } else { LABEL_FADE_S };
        if let Some(previous) = processing_before
            && (terminal || changing_phase)
            && elapsed < catch_up + transition
        {
            let blend = ((elapsed - catch_up) / transition).clamp(0., 1.);
            let old = self.interactive_layer(p, clock, previous.clone(), self.progress(), cx);
            if blend == 0. {
                return old;
            }
            let next =
                self.interactive_layer(p, clock, self.state.clone(), if terminal { None } else { self.progress() }, cx);
            return crossfade(old, next, blend);
        }
        if let Some(previous) = self.previous.as_ref().filter(|previous| **previous != self.state)
            && elapsed < STATE_FADE_S
        {
            let old = self
                .layer(p, clock, previous.clone(), self.previous_fill)
                .element(&mut |b| buttons::static_button(b, Scale::HUD, p).into_any_element());
            let next = self.interactive_layer(p, clock, self.state.clone(), self.progress(), cx);
            let u = (elapsed / STATE_FADE_S).clamp(0., 1.);
            return crossfade(old, next, u * u * (3. - 2. * u));
        }
        self.interactive_layer(p, clock, self.state.clone(), self.progress().filter(|_| !terminal), cx)
    }

    fn layer(&self, p: &Palette, clock: FrameClock, state: HudState, fill: Option<f32>) -> Capsule {
        let motion = Motion::system();
        Capsule {
            state,
            lang: self.st.cfg.get().general.lang(),
            scale: Scale::HUD,
            palette: *p,
            motion,
            w: self.width(),
            t: clock.scene,
            wave: if self.freeze.is_some() { Wave::Rest } else { Wave::Ocean(self.waveform.frame()) },
            fill,
            outcome_t: self.freeze.map_or_else(
                || Some((self.outcome_elapsed(clock.elapsed) - OUTCOME_ICON_DELAY).max(0.)),
                |t| (t > 0.).then_some(t),
            ),
        }
    }

    fn interactive_layer(
        &self,
        p: &Palette,
        clock: FrameClock,
        state: HudState,
        fill: Option<f32>,
        cx: &mut Context<Self>,
    ) -> Div {
        self.layer(p, clock, state, fill)
            .element(&mut |b| self.circle_button(b.look(p), b.on_press(), cx).into_any_element())
    }
}
