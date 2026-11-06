//! The capsule as the HUD view draws it: one layer, or two cross-fading while the state changes.

use dito_core::controller::HudState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hud::Scale;
use crate::hud::buttons;
use crate::hud::capsule::{Capsule, Wave};
use crate::hud::timing::{COPY_FADE_S, COPY_HOLD_END, OUTCOME_FADE_S, OUTCOME_HOLD_END, OUTCOME_ICON_DELAY, SETTLE_S};
use crate::theme::Palette;
use crate::ui::Motion;

use super::{HudView, LABEL_FADE_S};

const SUCCESS_FADE_S: f32 = 0.20;

/// The two clocks every layer of one frame reads, sampled once so cross-fading layers agree.
#[derive(Clone, Copy)]
struct FrameClock {
    elapsed: f32,
    scene: f32,
}

fn crossfade(old: Div, next: Div, blend: f32) -> Div {
    div().relative().child(old).child(next.absolute().top_0().left_0().opacity(blend))
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
            cap = cap.opacity((1. - (elapsed - hold_end) / fade).clamp(0., 1.));
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
        let changing_phase = self.state == HudState::Polishing && self.carry.is_some();
        let transition = if terminal { SUCCESS_FADE_S } else { LABEL_FADE_S };
        if let Some(previous) = processing_before
            && (terminal || changing_phase)
            && elapsed < SETTLE_S + transition
        {
            let blend = ((elapsed - SETTLE_S) / transition).clamp(0., 1.);
            let old = self.interactive_layer(p, clock, previous.clone(), self.progress(), cx);
            let next =
                self.interactive_layer(p, clock, self.state.clone(), if terminal { None } else { self.progress() }, cx);
            return crossfade(old, next, blend);
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
            wave: if motion.is_reduced() || self.freeze.is_some() {
                Wave::Rest
            } else {
                Wave::Bars(self.waveform.heights())
            },
            fill,
            outcome_t: self
                .freeze
                .map_or_else(|| Some((clock.elapsed - OUTCOME_ICON_DELAY).max(0.)), |t| (t > 0.).then_some(t)),
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
