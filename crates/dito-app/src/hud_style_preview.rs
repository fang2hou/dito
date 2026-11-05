//! A purely visual HUD demonstration. The view has no recorder, controller, sink, or history handle.

use std::time::{Duration, Instant};

use dito_core::config::{Appearance, RecordMode};
use dito_core::controller::HudState;
use dito_core::i18n::Lang;
use gpui_kit::*;

use crate::hud::{self, Capsule, Scale, Wave};

const LISTEN_END: f32 = 1.6;
const RECOGNIZE_END: f32 = 2.5;
const POLISH_END: f32 = 3.4;
const END: f32 = POLISH_END + hud::OUTCOME_END;

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
        let slot = || {
            div()
                .w(Scale::HUD.px(hud::CAPSULE_W))
                .h(Scale::HUD.px(hud::CAPSULE_H))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
        };
        if !self.active() {
            return slot();
        }
        let time = self.opened.elapsed().as_secs_f32();
        let reduced = dito_platform::system::reduce_motion();
        let palette = crate::theme::palette(self.appearance, cx);
        crate::ui::set_ui_language(self.language);
        let (state, start) = phase(time);
        let recognizing_end = hud::fill_at(&HudState::Recognizing, RECOGNIZE_END - LISTEN_END, None);
        let polishing_end = hud::fill_at(&HudState::Polishing, POLISH_END - RECOGNIZE_END, recognizing_end);
        let build = |state: HudState, since: f32, completion: Option<f32>| {
            let fill = if state == HudState::Polishing {
                hud::fill_at(&state, since, recognizing_end)
            } else {
                hud::fill_at(&state, since, None)
            };
            Capsule {
                state,
                lang: self.language,
                scale: Scale::HUD,
                palette,
                reduced,
                w: hud::CAPSULE_W,
                t: time,
                wave: if reduced { Wave::Rest } else { Wave::Bars(crate::hud::waveform::Waveform::demo(time)) },
                fill,
                outcome_t: completion,
            }
            .element(&mut |button| {
                hud::static_button(button, Scale::HUD, &palette).shadow(Vec::new()).into_any_element()
            })
            .shadow(Vec::new())
        };
        let elapsed = time - start;
        let terminal = state == HudState::Inserted;
        let mut capsule = if terminal {
            let blend = ((elapsed - 0.18) / 0.20).clamp(0., 1.);
            let result = build(state, elapsed, Some((elapsed - hud::OUTCOME_ICON_DELAY).max(0.)));
            if blend < 1. {
                // Complete the old fill under the same success background transition as the real HUD.
                let mut old = Capsule {
                    state: HudState::Polishing,
                    lang: self.language,
                    scale: Scale::HUD,
                    palette,
                    reduced,
                    w: hud::CAPSULE_W,
                    t: time,
                    wave: Wave::Rest,
                    fill: polishing_end.map(|from| {
                        let u = (elapsed / 0.18).clamp(0., 1.);
                        from + (1. - from) * (1. - (1. - u).powi(3))
                    }),
                    outcome_t: None,
                }
                .element(&mut |button| {
                    hud::static_button(button, Scale::HUD, &palette).shadow(Vec::new()).into_any_element()
                })
                .shadow(Vec::new());
                old = old.relative();
                div().relative().child(old).child(result.absolute().top_0().left_0().opacity(blend))
            } else {
                result
            }
        } else {
            build(state, elapsed, None)
        };
        if terminal {
            capsule = capsule.opacity(
                (1. - (elapsed - (hud::OUTCOME_END - hud::OUTCOME_FADE_S)) / hud::OUTCOME_FADE_S).clamp(0., 1.),
            );
        } else if time < hud::POP_MS as f32 / 1000. {
            capsule = hud::pop_shape(capsule, Scale::HUD, hud::CAPSULE_W, time / (hud::POP_MS as f32 / 1000.), reduced);
        }
        // Only the capsule. No language selector, device message, tooltip, buttons or keyboard actions above it.
        slot().child(capsule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn preview_runs_once_through_the_visual_stages() {
        assert!(matches!(phase(0.).0, HudState::Listening { .. }));
        assert_eq!(phase(1.6), (HudState::Recognizing, 1.6));
        assert_eq!(phase(2.5), (HudState::Polishing, 2.5));
        assert_eq!(phase(3.4), (HudState::Inserted, 3.4));
        assert!((END - 4.48).abs() < 0.001);
    }
}
