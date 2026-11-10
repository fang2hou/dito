//! The capsule: geometry, state colors and the content each state draws.

use dito_core::config::RecordMode;
use dito_core::controller::{ErrorAction, HudState};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, Palette, hex};
use crate::ui::{self, Motion};

use super::Scale;
use super::buttons::Btn;
use super::elevation::{self, Elevation};
use super::motion::{self, EntryPose};
use super::outcome_icon::{self, Outcome};
use super::progress;
use super::shimmer;
use super::timing;
use super::waveform::{self, OceanFrame};

pub(crate) const CAPSULE_W: f32 = 105.0;
/// Hold mode has no buttons, so the listening capsule is narrower. It widens to [`CAPSULE_W`] when recording ends.
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
    width_at(motion::Spring { position: from, velocity: 0. }, to, since).position
}

pub(super) fn width_at(from: motion::Spring, to: f32, since: f32) -> motion::Spring {
    if since >= WIDEN_S {
        return motion::Spring { position: to, velocity: 0. };
    }
    let peak = to + (to - from.position) * 0.097;
    let (a, b, velocity, local) = if since < 0.15 {
        (from.position, peak, from.velocity, since / 0.15)
    } else {
        (peak, to, 0., (since - 0.15) / 0.15)
    };
    let u = local.clamp(0., 1.);
    motion::Spring {
        position: a + (b - a) * motion::smooth(u) + velocity * 0.15 * u * (1. - u).powi(2),
        velocity: (b - a) * 6. * u * (1. - u) / 0.15 + velocity * (1. - u) * (1. - 3. * u),
    }
}

/// A live or simulated frame of the same listening surface.
#[derive(Clone, Copy)]
pub(crate) enum Wave {
    Ocean(OceanFrame),
    Rest,
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
    pub entry: EntryPose,
}

impl Capsule {
    /// The capsule. `buttons` supplies every round button it carries: the HUD window passes live ones,
    /// a still picture passes [`static_button`]s.
    pub(crate) fn element(mut self, buttons: &mut dyn FnMut(Btn, Scale) -> AnyElement) -> Div {
        let scale = self.scale;
        let (rest_width, entry) = (self.w, self.entry);
        self.w *= entry.width;
        let height = CAPSULE_H * entry.height;
        let mut cap = self.shell(height);
        if let Some(frac) = self.fill.filter(|&frac| frac > 0.) {
            let near_ceiling = match self.state {
                HudState::Transcribing => (0.47..=0.485).contains(&frac),
                HudState::Polishing => (0.97..=0.985).contains(&frac),
                _ => false,
            };
            let halo = if near_ceiling && !self.motion.is_reduced() { 1. + 0.20 * (self.t * 4.).sin() } else { 1. };
            cap = cap.child(progress::progress_layer(frac, self.w, scale, &self.palette, halo));
        }
        if let HudState::Listening { mode } = self.state {
            let frame = match self.wave {
                Wave::Ocean(frame) => frame,
                Wave::Rest => OceanFrame::default(),
            };
            cap = cap.child(waveform::ocean(
                frame,
                scale,
                self.palette.dark,
                self.motion.is_reduced(),
                rest_width,
                mode == RecordMode::Toggle,
                entry,
            ));
        }
        let width = self.w;
        let content = self
            .with_content(cap, height, buttons)
            .absolute()
            .left(scale.px((rest_width - width) / 2.))
            .top(scale.px((CAPSULE_H - height) / 2. + entry.y))
            .opacity(entry.alpha);
        div().relative().w(scale.px(rest_width)).h(scale.px(CAPSULE_H)).flex_none().child(content)
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
            .px(px(CAPSULE_PAD * self.entry.button))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(3.))
    }

    /// What the state puts inside the shell: buttons, waveform, label or icon.
    fn with_content(self, cap: Div, height: f32, buttons: &mut dyn FnMut(Btn, Scale) -> AnyElement) -> Div {
        let button_scale = Scale(self.scale.0 * self.entry.button);
        let Capsule { state, lang, scale, palette: p, motion, w, t: since, outcome_t, .. } = self;
        match state {
            HudState::Hidden => cap,
            HudState::Listening { mode: RecordMode::Toggle } => {
                cap.child(buttons(Btn::Cancel, button_scale)).child(buttons(Btn::Finish, button_scale))
            }
            HudState::Listening { mode: RecordMode::Hold } => cap,
            HudState::Transcribing => cap.justify_center().child(shimmer::shimmer_label(
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
                    ErrorAction::Retry => buttons(Btn::Retry, button_scale),
                    ErrorAction::OpenSettings => buttons(Btn::Settings, button_scale),
                };
                cap.child(buttons(Btn::Dismiss, button_scale))
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
        HudState::Listening { .. } => theme::hud_color(p, "#F7F5F0", "#444139"),
        _ => theme::hud_color(p, "#FFF9EB", "#444139"),
    };
    let (border_w, border_c) = match state {
        HudState::Error { .. } => (HAIRLINE, hex("#FF4949")),
        HudState::Inserted | HudState::Copied => (0., p.hud_border),
        _ => (HAIRLINE, p.hud_border),
    };
    (bg, border_w, border_c)
}

/// Only revealed glyph advances participate in the centered group. The baseline never moves.
fn copied_row(lang: Lang, scale: Scale, motion: Motion, w: f32, height: f32, outcome_t: Option<f32>) -> Div {
    let px = |v: f32| scale.px(v);
    let label = lang.tr(Tr::HudCopied);
    let size = scale.tx(timing::COPY_FONT);
    let glyphs: Vec<_> = label
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let value = ch.to_string();
            let width = ui::text_width(&value, timing::COPY_FONT, FontWeight::NORMAL);
            let (alpha, rise, advance) = timing::copied_glyph(outcome_t, i, motion);
            (value, width, alpha, rise, advance)
        })
        .collect();
    let (_, _, icon_advance) = timing::copied_sheets(outcome_t, motion);
    let gap = timing::COPY_GAP * glyphs.first().map_or(0., |glyph| glyph.4);
    let active_width = icon_advance + gap + glyphs.iter().map(|g| g.1 * g.4).sum::<f32>();
    let left = (w - active_width) / 2.;
    let mut row = div().absolute().left_0().top(px((height - 18.) / 2.)).w(px(w)).h(px(18.)).child(
        div().absolute().left(px(left)).top(px((18. - outcome_icon::COPY_ICON) / 2.)).child(outcome_icon::draw(
            Outcome::Copied,
            outcome_t,
            motion,
            scale,
        )),
    );
    let mut x = left + icon_advance + gap;
    for (value, width, alpha, rise, advance) in glyphs {
        let column = width * advance;
        let glyph = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let run = TextRun {
                    len: value.len(),
                    font: font(if ui::is_cjk(&value) { ui::cjk_family() } else { "Outfit" }),
                    color: hex("#FFF9EB").opacity(alpha),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(value.clone().into(), gpui_kit::px(size), &[run], None);
                let origin = point(
                    bounds.center().x - line.width() / 2.,
                    bounds.origin.y + gpui_kit::px((scale.tx(18.) - size) / 2.),
                );
                line.paint(origin, gpui_kit::px(size), TextAlign::Left, None, window, cx).ok();
            },
        )
        .absolute()
        .left(px(x))
        .top(px(rise))
        .w(px(column))
        .h(px(18.));
        row = row.child(glyph);
        x += column;
    }
    row
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
        for state in [HudState::Transcribing, HudState::Polishing, HudState::Inserted, HudState::Copied] {
            assert_eq!(rest_width(&state), CAPSULE_W, "{state:?}");
        }
    }
}
