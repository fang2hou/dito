//! The capsule: geometry, state colors and the content each state draws.

use dito_core::config::RecordMode;
use dito_core::controller::{ErrorAction, HudState};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, Palette, hex};
use crate::ui::{self, Motion, text};

use super::Scale;
use super::buttons::Btn;
use super::elevation::{self, Elevation};
use super::outcome_icon::{self, Outcome};
use super::progress;
use super::shimmer;
use super::timing;
use super::waveform::REST;

pub(crate) const CAPSULE_W: f32 = 105.0;
/// Hold mode has no buttons, so while listening the capsule is only as wide as its waveform needs; it widens to
/// [`CAPSULE_W`] when the recording ends.
pub(crate) const NARROW_W: f32 = 72.0;
/// The narrow → full widening, in seconds (ease-out).
pub(crate) const WIDEN_S: f32 = 0.3;
pub(crate) const CAPSULE_H: f32 = 33.0;
pub(super) const CAPSULE_PAD: f32 = 5.0;
/// Hairline border used by the capsule.
pub(super) const HAIRLINE: f32 = 1.0;

/// HUD text: 8% tracking for Japanese and Chinese, none for English (see `ui::tracking_for`).
fn spaced(s: &str, size: f32, weight: FontWeight, color: Hsla, scale: Scale) -> Div {
    ui::text_tracked(s, scale.tx(size), weight, color, 0.08)
}

/// Capsule width in design points once its state has settled: only hold-mode listening is narrow.
pub(crate) fn rest_width(state: &HudState) -> f32 {
    if matches!(state, HudState::Listening { mode: RecordMode::Hold }) { NARROW_W } else { CAPSULE_W }
}

/// The width `since` seconds into a spring from `from` to `to` (hold mode's capsule widening on release).
pub(crate) fn eased_width(from: f32, to: f32, since: f32) -> f32 {
    if since >= WIDEN_S - 0.000_01 {
        return to;
    }
    let u = (since / WIDEN_S).clamp(0., 1.);
    from + (to - from) * ui::spring(u, Motion::Full)
}

/// Smoothed microphone-driven heights, or the static design under Reduce Motion.
#[derive(Clone, Copy)]
pub(crate) enum Wave {
    Bars([f32; 8]),
    Rest,
}

fn waveform(wave: Wave, scale: Scale, p: &Palette) -> Div {
    let px = |v: f32| scale.px(v);
    let heights = match wave {
        Wave::Bars(heights) => heights,
        Wave::Rest => REST,
    };
    let mut row = div().flex().items_center().gap(px(2.)).h(px(19.));
    for h in heights {
        row = row
            .child(div().flex_none().w(px(2.)).h(px(h)).rounded_full().bg(theme::hud_color(p, "#6A6455", "#D4D1C9")));
    }
    row
}

/// Everything one capsule shows, so the HUD window and the setup wizard's usage demo draw the very same capsule.
pub(crate) struct Capsule {
    pub state: HudState,
    pub lang: Lang,
    pub scale: Scale,
    pub palette: Palette,
    pub motion: Motion,
    /// Outer width in design points: [`rest_width`], or on its way there while widening.
    pub w: f32,
    /// Shared shimmer clock in seconds; stage changes do not restart the sweep.
    pub t: f32,
    pub wave: Wave,
    /// Progress fill, 0–1 (see [`fill_at`]).
    pub fill: Option<f32>,
    /// None renders the settled icon for static previews.
    pub outcome_t: Option<f32>,
}

impl Capsule {
    /// The capsule. `buttons` supplies every round button it carries: the HUD window passes live ones,
    /// a still picture passes [`static_button`]s.
    pub(crate) fn element(self, buttons: &mut dyn FnMut(Btn) -> AnyElement) -> Div {
        let scale = self.scale;
        let copy_overshoot = if matches!(self.state, HudState::Copied) {
            (timing::copied_progress(self.outcome_t, self.motion) - 1.).max(0.)
        } else {
            0.
        };
        let height = CAPSULE_H - (copy_overshoot * 3.2).min(0.4);
        let mut cap = self.shell(height);
        if let Some(frac) = self.fill {
            cap = cap.child(progress::progress_layer((self.w * frac).clamp(0., self.w), self.w, scale, &self.palette));
        }
        self.with_content(cap, height, buttons)
    }

    fn shell(&self, height: f32) -> Div {
        let (scale, p) = (self.scale, &self.palette);
        let px = |v: f32| scale.px(v);
        let (bg, border_w, border_c) = surface_colors(&self.state, p);
        div()
            .w(px(self.w))
            .h(px(height))
            .rounded(px(height / 2.))
            .bg(bg)
            .border(px(border_w))
            .border_color(border_c)
            .shadow(elevation::surface_shadow(p, scale, Elevation::Surface))
            .overflow_hidden()
            .relative()
            .px(px(CAPSULE_PAD))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(3.))
    }

    /// What the state puts inside the shell: buttons, waveform, label or icon.
    fn with_content(self, cap: Div, height: f32, buttons: &mut dyn FnMut(Btn) -> AnyElement) -> Div {
        let Capsule { state, lang, scale, palette: p, motion, w, t: since, wave, outcome_t, .. } = self;
        match state {
            HudState::Hidden => cap,
            HudState::Listening { mode: RecordMode::Toggle } => cap
                .child(buttons(Btn::Cancel))
                .child(div().flex_1().flex().justify_center().child(waveform(wave, scale, &p)))
                .child(buttons(Btn::Finish)),
            HudState::Listening { mode: RecordMode::Hold } => cap.justify_center().child(waveform(wave, scale, &p)),
            HudState::Recognizing => cap.justify_center().child(shimmer::shimmer_label(
                lang.tr(Tr::HudTranscribing),
                since,
                scale,
                &p,
                motion,
            )),
            HudState::Polishing => {
                cap.justify_center().child(shimmer::shimmer_label(lang.tr(Tr::HudPolishing), since, scale, &p, motion))
            }
            HudState::Inserted => {
                cap.justify_center().child(outcome_icon::draw(Outcome::Inserted, outcome_t, motion, scale))
            }
            HudState::Copied => cap.child(copied_row(lang, scale, motion, w, height, outcome_t)),
            HudState::Error { action, .. } => {
                let right = match action {
                    ErrorAction::Retry => buttons(Btn::Retry),
                    ErrorAction::OpenSettings => buttons(Btn::Settings),
                };
                cap.child(buttons(Btn::Dismiss))
                    .child(spaced(
                        lang.tr(Tr::HudError),
                        12.,
                        FontWeight::NORMAL,
                        theme::hud_color(&p, "#FF4949", "#FF7972"),
                        scale,
                    ))
                    .child(right)
            }
        }
    }
}

/// Outcome capsules take their state color; the error one is the inverse danger style (Red 80% in light, a dark Red
/// tint in dark, with a Red border and Red ink), never solid Red.
fn surface_colors(state: &HudState, p: &Palette) -> (Hsla, f32, Hsla) {
    let bg = match state {
        HudState::Inserted => hex("#28A788"),
        HudState::Copied => hex("#AA8C40"),
        HudState::Error { .. } => theme::hud_error_bg(p),
        _ => theme::hud_color(p, "#FFF9EB", "#444139"),
    };
    let (border_w, border_c) = match state {
        HudState::Error { .. } => (HAIRLINE, hex("#FF4949")),
        HudState::Inserted | HudState::Copied => (0., p.hud_border),
        _ => (HAIRLINE, p.hud_border),
    };
    (bg, border_w, border_c)
}

/// The copied icon and label, which grow in together. Full-size horizontal slots are kept throughout; only opacity
/// and Y move per glyph.
fn copied_row(lang: Lang, scale: Scale, motion: Motion, w: f32, height: f32, outcome_t: Option<f32>) -> Div {
    let px = |v: f32| scale.px(v);
    let label = lang.tr(Tr::HudCopied);
    let size = scale.tx(12.);
    let full_width = scale.tx(timing::copied_label_width(lang));
    let progress = timing::copied_progress(outcome_t, motion);
    let icon_left = (scale.tx(w - outcome_icon::COPY_ICON) / 2.) - (full_width + scale.tx(4.)) / 2. * progress;
    let mut row = div().absolute().left_0().top(px((height - 18.) / 2.)).w(px(w)).h(px(18.));
    row = row.child(div().absolute().left(gpui_kit::px(icon_left)).top_0().child(outcome_icon::draw(
        Outcome::Copied,
        outcome_t,
        motion,
        scale,
    )));
    let mut prefix = String::new();
    for (i, ch) in label.chars().enumerate() {
        let x = ui::text_width(&prefix, size, FontWeight::NORMAL);
        prefix.push(ch);
        let (opacity, rise) = timing::copied_glyph(outcome_t, i, motion);
        row = row.child(
            div()
                .absolute()
                .left(gpui_kit::px(icon_left + scale.tx(outcome_icon::COPY_ICON + 4.) + x))
                .top(px(rise))
                .opacity(opacity)
                .child(text(ch.to_string(), size, FontWeight::NORMAL, hex("#FFF9EB")).line_height(px(18.))),
        );
    }
    row
}

/// The capsule's entry: it arrives like a drop of liquid — narrower and taller at first, rising from below —
/// and springs into its exact shape (a plain ease under Reduce Motion). `t` runs 0–1 over [`POP_MS`]; `w` is the
/// capsule's width in design points once arrived.
pub(crate) fn pop_shape<E: Styled>(c: E, scale: Scale, w: f32, t: f32, motion: Motion) -> E {
    let s = ui::spring(t, motion);
    let h = CAPSULE_H * (1.18 - 0.18 * s);
    let fade = (t * 2.2).min(1.);
    c.w(scale.px(w * (0.68 + 0.32 * s)))
        .h(scale.px(h))
        .rounded(scale.px(h / 2.))
        .mb(scale.px(-18. * (1. - s)))
        .opacity(fade * fade)
}

#[cfg(test)]
mod tests {
    use dito_core::config::RecordMode;
    use dito_core::controller::HudState;

    use super::{CAPSULE_W, NARROW_W, rest_width};

    #[test]
    fn only_hold_mode_listening_is_narrow() {
        let listening = |mode| HudState::Listening { mode };
        assert_eq!(rest_width(&listening(RecordMode::Hold)), NARROW_W);
        assert_eq!(rest_width(&listening(RecordMode::Toggle)), CAPSULE_W);
        for state in [HudState::Recognizing, HudState::Polishing, HudState::Inserted, HudState::Copied] {
            assert_eq!(rest_width(&state), CAPSULE_W, "{state:?}");
        }
    }
}
