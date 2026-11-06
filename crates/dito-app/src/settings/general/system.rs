use std::rc::Rc;

use dito_core::config::{Appearance, DisplayLanguage, HudPosition};
use dito_core::controller::HudState;
use dito_core::i18n::{Lang, Tr};
use dito_platform::system;
use gpui_kit::*;

use super::page::GeneralPage;
use crate::hud::{self, StylePreview};
use crate::services;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui;

const LOOKS: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

impl GeneralPage {
    pub(super) fn system_group(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        ui::group(
            &p,
            c.t(Tr::GeneralSystem),
            vec![
                ui::row(&p, c.t(Tr::GeneralLaunchAtLogin), self.launch_at_login_switch(&p), false),
                ui::row(&p, c.t(Tr::GeneralAppearance), self.appearance_seg(c), false),
                ui::row(&p, c.t(Tr::GeneralHudAppearance), self.hud_appearance_control(c, cx), false),
                ui::row(&p, c.t(Tr::GeneralHudPosition), self.hud_position_seg(c), false),
                self.hud_height_row(c),
                ui::row(&p, c.t(Tr::GeneralDisplayLanguage), self.display_language_select(c, cx), true),
            ],
        )
    }

    pub(in crate::settings) fn set_hud_appearance(&mut self, appearance: Appearance, cx: &mut Context<Self>) {
        if self.st.cfg.get().general.hud_appearance == appearance {
            return;
        }
        services::save_config(&self.st, |cfg| cfg.general.hud_appearance = appearance);
        if self.st.cfg.get().general.hud_appearance != appearance {
            return;
        }
        self.hud_preview = if *self.st.dictation.hud().borrow() == HudState::Hidden {
            let language = self.st.cfg.get().general.lang();
            Some(cx.new(|cx| StylePreview::new(appearance, language, cx)))
        } else {
            None
        };
        cx.notify();
    }

    pub(in crate::settings) fn cancel_hud_preview(&mut self, cx: &mut Context<Self>) {
        if self.hud_preview.take().is_some() {
            cx.notify();
        }
    }

    fn launch_at_login_switch(&self, p: &Palette) -> Stateful<Div> {
        let st = self.st.clone();
        ui::switch(
            "login",
            p,
            self.st.cfg.get().general.launch_at_login,
            Rc::new(move |on, _, _| {
                if !st.opts.mock
                    && let Err(e) = system::set_launch_at_login(on)
                {
                    tracing::warn!("login item: {e}");
                }
                services::save_config(&st, move |c| c.general.launch_at_login = on)
            }),
        )
    }

    fn appearance_seg(&self, c: &Ctx) -> Stateful<Div> {
        let st = self.st.clone();
        ui::appearance_seg(
            "appearance",
            &c.p,
            &[c.t(Tr::GeneralSystem), c.t(Tr::GeneralLight), c.t(Tr::GeneralDark)],
            LOOKS.iter().position(|a| *a == self.st.cfg.get().general.appearance).unwrap_or(0),
            Rc::new(move |i, _, _| services::save_config(&st, move |c| c.general.appearance = LOOKS[i])),
        )
    }

    fn hud_position_seg(&self, c: &Ctx) -> Stateful<Div> {
        let st = self.st.clone();
        ui::seg(
            "hud-position",
            &c.p,
            &[c.t(Tr::GeneralHudCentered), c.t(Tr::GeneralHudPointer)],
            usize::from(self.st.cfg.get().general.hud_position == HudPosition::Pointer),
            Rc::new(move |i, _, _| {
                services::save_config(&st, move |c| {
                    c.general.hud_position = if i == 0 { HudPosition::Centered } else { HudPosition::Pointer };
                })
            }),
        )
    }

    fn display_language_select(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let st = self.st.clone();
        ui::select(
            "ui-lang",
            &c.p,
            &[
                c.t(Tr::GeneralFollowSystem),
                SharedString::from(Lang::Ja.native_name()),
                SharedString::from(Lang::En.native_name()),
                SharedString::from(Lang::Zh.native_name()),
            ],
            DisplayLanguage::ALL.iter().position(|l| *l == self.st.cfg.get().general.display_language).unwrap_or(0),
            self.open == Some("ui-lang"),
            Some(240.),
            self.toggle(cx, "ui-lang"),
            Rc::new(move |i, _, _| {
                services::save_config(&st, move |c| c.general.display_language = DisplayLanguage::ALL[i])
            }),
        )
    }

    /// The live preview of the HUD's look beside the control that picks it.
    fn hud_appearance_control(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let weak = cx.entity().downgrade();
        let hud_switch = ui::appearance_seg(
            "hud-appearance",
            &p,
            &[c.t(Tr::GeneralSystem), c.t(Tr::GeneralLight), c.t(Tr::GeneralDark)],
            LOOKS.iter().position(|a| *a == self.st.cfg.get().general.hud_appearance).unwrap_or(0),
            Rc::new(move |i, _, app| {
                let _ = weak.update(app, |page, cx| page.set_hud_appearance(LOOKS[i], cx));
            }),
        );
        div()
            .flex()
            .items_center()
            .gap(px(24.))
            .child(
                div()
                    .w(px(hud::CAPSULE_W * hud::HUD_SCALE))
                    .h(px(hud::CAPSULE_H * hud::HUD_SCALE))
                    .flex_none()
                    .children(self.hud_preview.clone()),
            )
            .child(hud_switch)
    }

    /// The height slider's row unfolds only while the HUD is centered.
    fn hud_height_row(&self, c: &Ctx) -> Div {
        let p = c.p;
        let hud_height = self.hud_height.clone();
        let height_title = c.t(Tr::GeneralHudHeight);
        let target = if self.st.cfg.get().general.hud_position == HudPosition::Centered { 1. } else { 0. };
        let duration = if system::reduce_motion() { 1 } else { 180 };
        div().w_full().child(ui::tweened(ElementId::from("hud-height-row"), target, duration, move |v| {
            let row = div().w_full().h(px(64. * v)).overflow_hidden().opacity(v);
            if v > 0.001 { row.child(ui::row(&p, height_title.clone(), hud_height.clone(), false)) } else { row }
        }))
    }
}
