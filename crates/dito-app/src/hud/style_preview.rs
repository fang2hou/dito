//! A purely visual HUD demonstration. The view has no recorder, controller, sink, or history handle.

use std::time::{Duration, Instant};

use dito_core::config::{Appearance, RecordMode};
use dito_core::controller::HudState;
use dito_core::i18n::Lang;
use gpui_kit::*;

use crate::theme::{self, Palette};
use crate::ui::{self, Motion};

use super::waveform::Waveform;
use super::{CAPSULE_H, CAPSULE_W, Capsule, OUTCOME_END, OUTCOME_FADE_S, OUTCOME_ICON_DELAY, POP_MS, Scale, Wave};
use super::{fill_at, pop_shape, static_button};

const LISTEN_END: f32 = 1.6;
const RECOGNIZE_END: f32 = 2.5;
const POLISH_END: f32 = 3.4;
const END: f32 = POLISH_END + OUTCOME_END;

fn phase(t: f32) -> (HudState, f32) {
    if t < LISTEN_END {
        (HudState::Listening { mode: RecordMode::Toggle }, 0.)
    } else if t < RECOGNIZE_END {
        (HudState::Recognizing, LISTEN_END)
    } else if t < POLISH_END {
        (HudState::Polishing, RECOGNIZE_END)
    } else {
        (HudState::Inserted, POLISH_END)
    }
}

pub(crate) struct StylePreview {
    appearance: Appearance,
    language: Lang,
    opened: Instant,
}

impl StylePreview {
    pub(crate) fn new(appearance: Appearance, language: Lang, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let active = this.update(cx, |view, cx| {
                    cx.notify();
                    view.active()
                });
                if !active.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
        Self { appearance, language, opened: Instant::now() }
    }

    pub(crate) fn active(&self) -> bool {
        self.opened.elapsed().as_secs_f32() < END
    }
}

impl Render for StylePreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slot = div()
            .w(Scale::HUD.px(CAPSULE_W))
            .h(Scale::HUD.px(CAPSULE_H))
            .flex_none()
            .flex()
            .items_center()
            .justify_center();
        if !self.active() {
            return slot;
        }
        let frame = Frame {
            time: self.opened.elapsed().as_secs_f32(),
            language: self.language,
            palette: theme::palette(self.appearance, cx),
            motion: Motion::system(),
        };
        ui::set_ui_language(self.language);
        // Only the capsule. No language selector, device message, tooltip, buttons or keyboard actions above it.
        slot.child(frame.scene())
    }
}

fn recognizing_end() -> Option<f32> {
    fill_at(&HudState::Recognizing, RECOGNIZE_END - LISTEN_END, None)
}

fn polishing_end() -> Option<f32> {
    fill_at(&HudState::Polishing, POLISH_END - RECOGNIZE_END, recognizing_end())
}

struct Frame {
    time: f32,
    language: Lang,
    palette: Palette,
    motion: Motion,
}

impl Frame {
    fn scene(&self) -> Div {
        let (state, start) = phase(self.time);
        let elapsed = self.time - start;
        if state == HudState::Inserted {
            return self.inserted(elapsed);
        }
        let capsule = self.capsule(state, elapsed, None);
        let pop_s = POP_MS as f32 / 1000.;
        if self.time < pop_s {
            return pop_shape(capsule, Scale::HUD, CAPSULE_W, self.time / pop_s, self.motion);
        }
        capsule
    }

    fn inserted(&self, elapsed: f32) -> Div {
        let blend = ((elapsed - 0.18) / 0.20).clamp(0., 1.);
        let result = self.capsule(HudState::Inserted, elapsed, Some((elapsed - OUTCOME_ICON_DELAY).max(0.)));
        let capsule = if blend < 1. {
            // Complete the old fill under the same success background transition as the real HUD.
            let old = self.completing_polish(elapsed).relative();
            div().relative().child(old).child(result.absolute().top_0().left_0().opacity(blend))
        } else {
            result
        };
        capsule.opacity((1. - (elapsed - (OUTCOME_END - OUTCOME_FADE_S)) / OUTCOME_FADE_S).clamp(0., 1.))
    }

    fn capsule(&self, state: HudState, since: f32, completion: Option<f32>) -> Div {
        let carried = if state == HudState::Polishing { recognizing_end() } else { None };
        let fill = fill_at(&state, since, carried);
        let wave = if matches!(state, HudState::Listening { .. }) {
            Wave::Ocean(Waveform::demo(self.time))
        } else {
            Wave::Rest
        };
        self.element(state, wave, fill, completion)
    }

    fn completing_polish(&self, elapsed: f32) -> Div {
        let fill = polishing_end().map(|from| {
            let u = (elapsed / 0.18).clamp(0., 1.);
            from + (1. - from) * (1. - (1. - u).powi(3))
        });
        self.element(HudState::Polishing, Wave::Rest, fill, None)
    }

    fn element(&self, state: HudState, wave: Wave, fill: Option<f32>, outcome_t: Option<f32>) -> Div {
        let palette = self.palette;
        Capsule {
            state,
            lang: self.language,
            scale: Scale::HUD,
            palette,
            motion: self.motion,
            w: CAPSULE_W,
            t: self.time,
            wave,
            fill,
            outcome_t,
        }
        .element(&mut |button| static_button(button, Scale::HUD, &palette).shadow(Vec::new()).into_any_element())
        .shadow(Vec::new())
    }
}
