use std::rc::Rc;

use dito_core::config::{InsertMethod, MusicMuteMode, RecordMode};
use dito_core::i18n::Tr;
use dito_platform::hotkey::Shortcut;
use gpui_kit::*;

use super::page::GeneralPage;
use crate::services;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, text};

impl GeneralPage {
    pub(super) fn input_group(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        ui::group(
            &p,
            c.t(Tr::GeneralInput),
            vec![
                ui::row(&p, c.t(Tr::GeneralRecord), self.shortcut_field(c, cx), false),
                ui::row(&p, c.t(Tr::GeneralMode), self.record_mode_seg(c), false),
                ui::row(&p, c.t(Tr::GeneralInsertMethod), self.insert_method_seg(c), false),
                ui::row(&p, c.t(Tr::ModelsPolish), self.polish_switch(&p), false),
                ui::row(&p, c.t(Tr::GeneralMuteMusic), self.music_mute_seg(c), true),
            ],
        )
    }

    fn record_mode_seg(&self, c: &Ctx) -> Stateful<Div> {
        let st = self.st.clone();
        ui::seg(
            "mode",
            &c.p,
            &[c.t(Tr::GeneralToggle), c.t(Tr::GeneralHold)],
            if self.st.cfg.get().general.record_mode == RecordMode::Toggle { 0 } else { 1 },
            Rc::new(move |i, _, _| {
                services::save_config(&st, move |c| {
                    c.general.record_mode = if i == 0 { RecordMode::Toggle } else { RecordMode::Hold }
                })
            }),
        )
    }

    fn insert_method_seg(&self, c: &Ctx) -> Stateful<Div> {
        let st = self.st.clone();
        ui::seg(
            "insert",
            &c.p,
            &[c.t(Tr::GeneralKeyInput), c.t(Tr::GeneralPaste)],
            if self.st.cfg.get().general.insert_method == InsertMethod::KeyInput { 0 } else { 1 },
            Rc::new(move |i, _, _| {
                services::save_config(&st, move |c| {
                    c.general.insert_method = if i == 0 { InsertMethod::KeyInput } else { InsertMethod::Paste }
                })
            }),
        )
    }

    fn music_mute_seg(&self, c: &Ctx) -> Stateful<Div> {
        let st = self.st.clone();
        ui::seg(
            "music-mute",
            &c.p,
            &[c.t(Tr::GeneralOff), c.t(Tr::GeneralAuto)],
            if self.st.cfg.get().general.music_mute == MusicMuteMode::Auto { 1 } else { 0 },
            Rc::new(move |i, _, _| {
                services::save_config(&st, move |c| {
                    c.general.music_mute = if i == 1 { MusicMuteMode::Auto } else { MusicMuteMode::Off }
                })
            }),
        )
    }

    fn polish_switch(&self, p: &Palette) -> Stateful<Div> {
        let st = self.st.clone();
        ui::switch(
            "polish-enabled",
            p,
            self.st.cfg.get().polish.enabled,
            Rc::new(move |on, _, _| {
                services::save_config(&st, |c| c.polish.enabled = on);
            }),
        )
    }

    fn shortcut_field(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let field = if self.capturing { self.capturing_field(c) } else { self.idle_field(c, cx) };
        let hint = c.t(Tr::GeneralPressEscToCancel);
        // Static positioned box: an animated one re-lays out against the wrong ancestor. The symmetric box (non-zero
        // insets) centres the bubble without measuring the text, and the top offset clears the field. Deferred, so
        // following rows' separators cannot paint over the bubble.
        div().relative().child(field).child(deferred(
            div()
                .absolute()
                .top(px(-(ui::TIP_BUBBLE_H + 6.)))
                .left(px(0.5))
                .right(px(0.5))
                .flex()
                .justify_center()
                .child(ui::tip_pop(p, ElementId::from("shortcut-hint"), hint, self.capturing)),
        ))
    }

    fn capturing_field(&self, c: &Ctx) -> AnyElement {
        let p = &c.p;
        div()
            .id("shortcut-recording")
            .w(px(190.))
            .h(px(ui::CONTROL_H))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .rounded(px(12.))
            .bg(p.accent_soft)
            .border_2()
            .border_color(p.accent)
            .child(div().size(px(8.)).rounded_full().bg(p.danger))
            .child(
                text(c.t(Tr::GeneralWaitingForInput), 13., FontWeight::MEDIUM, p.accent_text).line_height(relative(1.)),
            )
            .into_any_element()
    }

    fn idle_field(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let p = &c.p;
        let shortcut = self.st.cfg.get().general.shortcut.clone();
        let label = Shortcut::parse(&shortcut)
            .map(|s| dito_platform::keymap::label(&s, c.lang))
            .unwrap_or_else(|| shortcut.clone());
        div()
            .id("shortcut-field")
            .w(px(190.))
            .h(px(ui::CONTROL_H))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .bg(p.input_bg)
            .border_1()
            .border_color(p.border_strong)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, _| this.st.hotkeys.begin_capture()))
            .child(text(label, 15., FontWeight::BOLD, p.text).line_height(relative(1.)))
            .into_any_element()
    }
}
