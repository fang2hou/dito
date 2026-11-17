use std::rc::Rc;

use dito_core::config::{Appearance, DisplayLanguage, HudPosition, PreferredLanguage};
use dito_core::controller::HudState;
use dito_core::i18n::{Lang, Tr};
use dito_platform::system;
use gpui_kit::*;

use super::page::GeneralPage;
use crate::hotkeys::ShortcutTarget;
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
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralLaunchAtLogin),
                    c.t(Tr::GeneralLaunchAtLoginHint),
                    self.launch_at_login_switch(&p),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralShowDockIcon),
                    c.t(Tr::GeneralShowDockIconHint),
                    self.show_dock_icon_switch(&p),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralDisplayLanguage),
                    c.t(Tr::GeneralDisplayLanguageHint),
                    self.display_language_select(c, cx),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralAppearance),
                    c.t(Tr::GeneralAppearanceHint),
                    self.appearance_seg(c),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralToolsShortcut),
                    c.t(Tr::GeneralToolsShortcutHint),
                    self.shortcut_field(c, cx, ShortcutTarget::Tools),
                    true,
                ),
            ],
        )
    }

    /// `design:settings/general`.
    pub(super) fn hud_group(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        ui::group(
            &p,
            c.t(Tr::GeneralHud),
            vec![
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralHudAppearance),
                    c.t(Tr::GeneralHudAppearanceHint),
                    self.hud_appearance_control(c, cx),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralHudPosition),
                    c.t(Tr::GeneralHudPositionHint),
                    self.hud_position_seg(c),
                    false,
                ),
                self.hud_height_row(c),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralHudLanguageShortcuts),
                    c.t(Tr::GeneralHudLanguageShortcutsHint),
                    self.hud_language_select(c, cx),
                    true,
                ),
            ],
        )
    }

    fn hud_language_select(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let st = self.st.clone();
        let cfg = st.cfg.get();
        let selected = PreferredLanguage::ALL.map(|language| cfg.general.hud_languages.contains(&language));
        let label = match selected.iter().filter(|&&on| on).count() {
            0 => c.t(Tr::GeneralHudLanguageHidden),
            count if count == selected.len() => c.t(Tr::GeneralHudLanguageAll),
            _ => {
                cfg.general.hud_languages().map(|language| language.name(c.lang)).collect::<Vec<_>>().join(", ").into()
            }
        };
        ui::select_multi(
            "hud-languages",
            &c.p,
            &PreferredLanguage::ALL.map(|language| language.name(c.lang).into()),
            &selected,
            label,
            self.open == Some("hud-languages"),
            240.,
            self.toggle(cx, "hud-languages"),
            Rc::new(move |i, _, _| {
                services::save_config(&st, |c| c.general.toggle_hud_language(PreferredLanguage::ALL[i]));
            }),
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

    fn show_dock_icon_switch(&self, p: &Palette) -> Stateful<Div> {
        let st = self.st.clone();
        ui::switch(
            "dock-icon",
            p,
            self.st.cfg.get().general.show_dock_icon,
            Rc::new(move |on, _, _| services::save_config(&st, move |c| c.general.show_dock_icon = on)),
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
        let mut options = Vec::with_capacity(1 + Lang::ALL.len());
        options.push(c.t(Tr::GeneralFollowSystem));
        options.extend(Lang::ALL.into_iter().map(|l| SharedString::from(l.native_name())));
        ui::select(
            "ui-lang",
            &c.p,
            &options,
            DisplayLanguage::ALL.iter().position(|l| *l == self.st.cfg.get().general.display_language).unwrap_or(0),
            self.open == Some("ui-lang"),
            Some(240.),
            self.toggle(cx, "ui-lang"),
            Rc::new(move |i, _, _| {
                services::save_config(&st, move |c| c.general.display_language = DisplayLanguage::ALL[i])
            }),
        )
    }

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

    fn hud_height_row(&self, c: &Ctx) -> Div {
        let p = c.p;
        let hud_height = self.hud_height.clone();
        let height_title = c.t(Tr::GeneralHudHeight);
        let height_hint = c.t(Tr::GeneralHudHeightHint);
        let target = if self.st.cfg.get().general.hud_position == HudPosition::Centered { 1. } else { 0. };
        let duration = if system::reduce_motion() { 1 } else { 180 };
        div().w_full().child(ui::tweened(ElementId::from("hud-height-row"), target, duration, move |v| {
            let row = div().relative().w_full().h(px(64. * v)).overflow_hidden();
            if v > 0.001 {
                // Fade the finished control against its surface, not each overlapping primitive.
                row.child(ui::row_described(&p, height_title.clone(), height_hint.clone(), hud_height.clone(), false))
                    .child(div().absolute().inset_0().bg(p.card.opacity(1. - v)))
            } else {
                row
            }
        }))
    }
}
