//! What the HUD draws around the capsule: tips, the stop pill, the error reason and the countdown.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::RecordMode;
use dito_core::controller::{Command, ErrorAction, HudState};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hud::buttons::{self, BTN};
use crate::hud::capsule::{CAPSULE_H, CAPSULE_PAD, CAPSULE_W};
use crate::hud::elevation::{self, Elevation};
use crate::hud::window::{WIN_PAD, WIN_W};
use crate::hud::{Scale, px, tx};
use crate::theme::{self, Palette};
use crate::ui::{self, text};

use super::clock::{COUNTDOWN_FADE_S, COUNTDOWN_START_S};
use super::layout::{CAPSULE_TOP, ERROR_BUBBLE_H, ZONE_GAP};
use super::{HudView, LABEL_FADE_S, PILL_HAIRLINE};

/// Centres of the capsule's left and right buttons from the window's left edge.
fn button_centers() -> (f32, f32) {
    // The capsule centres on the full window width (the root's horizontal padding does not shift that
    // centring), and a tip's absolute `left` is window-0-based, so anchors use that same frame.
    let capsule_left = (WIN_W + 2. * WIN_PAD - CAPSULE_W) / 2.;
    let button_center = CAPSULE_PAD + BTN / 2.;
    (capsule_left + button_center, capsule_left + CAPSULE_W - button_center)
}

impl HudView {
    pub(super) fn bubble(s: &str, p: &Palette) -> Div {
        div()
            .px(px(10.5))
            .py(px(5.5))
            .rounded(px(8.))
            .bg(p.hud_bg)
            .border(px(PILL_HAIRLINE))
            .border_color(p.hud_border)
            .shadow(elevation::surface_shadow(p, Scale::HUD, Elevation::Surface))
            .flex_none()
            .whitespace_nowrap()
            .child(text(s.to_string(), tx(12.), FontWeight::NORMAL, p.text).line_height(px(18.)))
    }

    /// The tips above the capsule's buttons, or the device note while one is shown.
    pub(super) fn tips_above(&self, lang: Lang, p: &Palette, error_detail: Option<&'static str>) -> Div {
        let listening = matches!(self.state, HudState::Listening { mode: RecordMode::Toggle });
        let device_note = self.device_note(lang);
        let note_taken = device_note.is_some();
        let (left, right) = button_centers();
        let mut above = div().w_full().flex_1().relative().flex().flex_col().justify_end().items_center();
        if let Some(note) = device_note.filter(|_| error_detail.is_none()) {
            above = above.child(Self::bubble(&note, p));
        }
        let tip_on =
            |btn: &'static str| listening && self.hover_btn == Some(btn) && error_detail.is_none() && !note_taken;
        above = above
            .child(self.button_tip("hud-tip-finish", right, lang.tr(Tr::HudFinish), tip_on("hud-finish"), p))
            .child(self.button_tip("hud-tip-cancel", left, lang.tr(Tr::HudCancel), tip_on("hud-cancel"), p));
        if let HudState::Error { action, .. } = &self.state {
            let tip_on = |btn: &'static str| {
                error_detail.is_some()
                    && self.hover_btn == Some(btn)
                    && (self.error_reason.position > 0.18 || self.freeze.is_some())
            };
            let (right_id, right_btn, right_text) = match action {
                ErrorAction::Retry => ("hud-tip-retry", "hud-retry", lang.tr(Tr::HudRetry)),
                ErrorAction::OpenSettings => ("hud-tip-settings", "hud-settings", lang.tr(Tr::HudOpenSettings)),
            };
            above = above
                .child(self.button_tip("hud-tip-close", left, lang.tr(Tr::HudClose), tip_on("hud-dismiss"), p))
                .child(self.button_tip(right_id, right, right_text, tip_on(right_btn), p));
        }
        above
    }

    fn device_note(&self, lang: Lang) -> Option<String> {
        self.st
            .capture
            .as_ref()
            .and_then(|c| c.device_notice())
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(1))
            .map(|(name, _)| dito_core::i18n::fmt::using_model(lang, &name))
    }

    /// The hover zone: the capsule plus the controls above it and the gap between, so the pointer can travel up.
    pub(super) fn zone(&self, p: &Palette, lang: Lang, capsule: AnyElement, cx: &mut Context<Self>) -> Stateful<Div> {
        let listening = matches!(self.state, HudState::Listening { mode: RecordMode::Toggle });
        let processing = matches!(self.state, HudState::Recognizing | HudState::Polishing);
        let mut zone = div().id("hud-zone").flex().flex_col().items_center().gap(px(ZONE_GAP));
        let chip_on = listening
            && self.hover_btn.is_none()
            && self.hover_zone
            && self.st.cfg.get().general.hud_languages().next().is_some();
        let pill_on = processing && self.hover_zone;
        let chip = self.language_chip(p, lang, cx);
        if let Some(chip) = ui::hover_pop_scaled(ElementId::from("hud-lang-chip"), chip_on, 0.8, chip) {
            zone = zone.child(chip);
        }
        if let Some(pill) = ui::hover_pop(ElementId::from("hud-stop-pill"), pill_on, || {
            let palette = *p;
            let hover = f32::from(self.hover_btn == Some("hud-stop"));
            div().id("hud-stop").cursor_pointer().on_click(cx.listener(|this, _, _, _| this.send(Command::Stop))).child(
                ui::tweened("hud-stop-appearance".into(), hover, 140, move |v| buttons::stop_pill(&palette, lang, v)),
            )
        }) {
            zone = zone.child(pill);
        }
        zone.child(capsule)
    }

    /// The reason of an error, travelling out from behind the capsule; its fixed anchor cannot move the HUD.
    pub(super) fn error_reason_bubble(&self, msg: &str, p: &Palette) -> Div {
        let progress = self.error_reason.position;
        let y = CAPSULE_TOP - ZONE_GAP - ERROR_BUBBLE_H + (CAPSULE_H + 2. * ZONE_GAP + ERROR_BUBBLE_H) * progress;
        div().absolute().top(px(y)).left(px(0.5)).right(px(0.5)).flex().justify_center().child(Self::bubble(msg, p))
    }

    /// The remaining recording time, with the seconds still to run: `active` while recording, fading out after.
    pub(super) fn countdown(&self, p: &Palette, seconds: u64, active: bool) -> Div {
        let reduced = dito_platform::system::reduce_motion();
        let elapsed = self.freeze.unwrap_or_else(|| self.recording_started.map_or(0., |t| t.elapsed().as_secs_f32()));
        let entry = ((elapsed - COUNTDOWN_START_S) / COUNTDOWN_FADE_S).clamp(0., 1.);
        let opacity = if active { entry } else { (1. - self.elapsed() / LABEL_FADE_S).clamp(0., 1.) };
        let (bg, border, ink) = if seconds <= 10 {
            (theme::hud_countdown_warn_bg(p), theme::hud_countdown_warn_border(p), theme::hud_countdown_warn_ink(p))
        } else {
            (p.hud_bg, p.hud_border, p.text)
        };
        let pill = div()
            .w(px(52.))
            .h(px(22.))
            .rounded_full()
            .border(px(PILL_HAIRLINE))
            .border_color(border)
            .bg(bg)
            .shadow(elevation::surface_shadow(p, Scale::HUD, Elevation::Surface))
            .flex()
            .items_center()
            .justify_center()
            .child(
                text(format!("{:02}:{:02}", seconds / 60, seconds % 60), tx(12.), FontWeight::NORMAL, ink)
                    .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)]))),
            );
        div()
            .absolute()
            .bottom(px(5. + WIN_PAD - if reduced || !active { 0. } else { (1. - entry) * 4. }))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .opacity(opacity)
            .child(pill)
    }

    /// A tooltip centred above the capsule button whose centre is `center_x` from the window's left edge, in the
    /// full-window frame. The anchor is a box placed symmetrically about that centre, so `justify_center` centres
    /// the bubble on the button without measuring the text: measured widths drift from the rendered run, which
    /// broke centring whenever the language changed.
    ///
    /// The anchor box stays mounted as a plain static element (an animated one re-lays out against the wrong
    /// ancestor); the pop lives in its inner child.
    fn button_tip(&self, id: &'static str, center_x: f32, s: &str, on: bool, p: &Palette) -> Div {
        let full = WIN_W + 2. * WIN_PAD;
        let r = center_x.min(full - center_x);
        // Both insets stay non-zero: a zero-valued `left` measures as if unset.
        div()
            .absolute()
            .bottom(px(0.))
            .left(px((center_x - r + 0.5).max(0.5)))
            .right(px((full - center_x - r - 0.5).max(0.5)))
            .flex()
            .justify_center()
            .child(if self.freeze.is_some() && matches!(self.state, HudState::Error { .. }) {
                Self::bubble(s, p).opacity(if on { 1. } else { 0. }).into_any_element()
            } else {
                ui::tip_pop_with(
                    p,
                    ElementId::from((ElementId::from(id), "tip")),
                    SharedString::from(s),
                    on,
                    p.hud_bg,
                    p.text,
                )
            })
    }
}
