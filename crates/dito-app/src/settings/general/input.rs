use std::rc::Rc;

use dito_core::config::{InsertMethod, MusicMuteMode, RecordMode};
use dito_core::i18n::Tr;
use dito_platform::hotkey::{Shortcut, command_shortcut};
use gpui_kit::*;

use super::page::GeneralPage;
use crate::assets::Ic;
use crate::hotkeys::{CaptureState, ShortcutTarget};
use crate::services;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, cbk, text};

impl GeneralPage {
    pub(super) fn input_group(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        ui::group(
            &p,
            c.t(Tr::GeneralDictation),
            vec![
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralShortcut),
                    c.t(Tr::GeneralShortcutHint),
                    self.shortcut_field(c, cx, ShortcutTarget::Dictation),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralKeyPress),
                    c.t(Tr::GeneralKeyPressHint),
                    self.record_mode_seg(c),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralInsertMethod),
                    c.t(Tr::GeneralInsertMethodHint),
                    self.insert_method_seg(c),
                    false,
                ),
                ui::row_described(&p, c.t(Tr::ModelsPolish), c.t(Tr::GeneralPolishHint), self.polish_switch(&p), false),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralMuteMusic),
                    c.t(Tr::GeneralMuteMusicHint),
                    self.music_mute_seg(c),
                    true,
                ),
            ],
        )
    }

    pub(super) fn command_group(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        ui::group(
            &p,
            c.t(Tr::GeneralCommand),
            vec![
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralShortcut),
                    c.t(Tr::GeneralCommandShortcutHint),
                    self.command_shortcut_control(c, cx),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralConfirmBeforeReplace),
                    c.t(Tr::GeneralConfirmBeforeReplaceHint),
                    self.confirm_before_replace_switch(&p),
                    true,
                ),
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

    fn confirm_before_replace_switch(&self, p: &Palette) -> Stateful<Div> {
        let st = self.st.clone();
        ui::switch(
            "confirm-before-replace",
            p,
            self.st.cfg.get().general.confirm_before_replace,
            Rc::new(move |on, _, _| {
                services::save_config(&st, |c| c.general.confirm_before_replace = on);
            }),
        )
    }

    /// The command shortcut field. While the user has set a shortcut, a reset button sits 8 pt left of it and gives
    /// back the default (Shift plus the dictation shortcut); it fades with `motion/hover` and never moves the field.
    fn command_shortcut_control(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let set = !self.st.cfg.get().general.command_shortcut.is_empty();
        let duration = if ui::Motion::system().is_reduced() { 80 } else { ui::HOVER_MS };
        let (opacity, fading) =
            ui::fade_value(&ElementId::from("command-shortcut-reset-fade"), f32::from(set), duration);
        if fading {
            self.fading.set(true);
        }
        let reset = (set || opacity > 0.).then(|| {
            let id = ElementId::from("command-shortcut-reset");
            let label = c.t(Tr::GeneralRestoreDefault);
            ui::tip_label(id.clone(), label.clone());
            let button = ui::icon_button_with(
                id.clone(),
                p,
                Ic::Reset,
                false,
                ui::CircleSize::Regular,
                cbk(cx, |this: &mut Self, _, cx| {
                    if this.capture.is_waiting_for(ShortcutTarget::Command) {
                        this.st.hotkeys.cancel_capture();
                    }
                    services::save_config(&this.st, |c| c.general.command_shortcut.clear());
                    cx.notify();
                }),
            )
            .role(Role::Button)
            .aria_label(label)
            .child(ui::tip_above(p, &id));
            div().opacity(opacity).child(button)
        });
        div().flex().items_center().gap(px(8.)).children(reset).child(self.shortcut_field(
            c,
            cx,
            ShortcutTarget::Command,
        ))
    }

    fn shortcut_field(&self, c: &Ctx, cx: &mut Context<Self>, target: ShortcutTarget) -> Div {
        let p = &c.p;
        let waiting = self.capture.is_waiting_for(target);
        let field = if waiting { self.capturing_field(c, target) } else { self.idle_field(c, cx, target) };
        let (hint, shown, hint_id) = match (target, self.capture) {
            (ShortcutTarget::Dictation, CaptureState::Refused(ShortcutTarget::Dictation)) => {
                (Tr::GeneralShortcutSameAsCommand, true, "shortcut-hint")
            }
            (ShortcutTarget::Command, CaptureState::Refused(ShortcutTarget::Command)) => {
                (Tr::GeneralShortcutSameAsDictation, true, "command-shortcut-hint")
            }
            (ShortcutTarget::Dictation, _) => (Tr::GeneralPressEscToCancel, waiting, "shortcut-hint"),
            (ShortcutTarget::Command, _) => (Tr::GeneralShortcutTakenHint, waiting, "command-shortcut-hint"),
        };
        let hint = c.t(hint);
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
                .child(ui::tip_pop(p, ElementId::from(hint_id), hint, shown)),
        ))
    }

    fn capturing_field(&self, c: &Ctx, target: ShortcutTarget) -> AnyElement {
        let p = &c.p;
        div()
            .id(match target {
                ShortcutTarget::Dictation => "shortcut-recording",
                ShortcutTarget::Command => "command-shortcut-recording",
            })
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

    fn idle_field(&self, c: &Ctx, cx: &mut Context<Self>, target: ShortcutTarget) -> AnyElement {
        let p = &c.p;
        let (id, label) = match target {
            ShortcutTarget::Dictation => {
                let shortcut = self.st.cfg.get().general.shortcut.clone();
                let label = Shortcut::parse(&shortcut)
                    .map(|s| dito_platform::keymap::label(&s, c.lang))
                    .unwrap_or_else(|| shortcut.clone());
                ("shortcut-field", label)
            }
            ShortcutTarget::Command => {
                let general = self.st.cfg.get().general.clone();
                let label = command_shortcut(&general.shortcut, &general.command_shortcut)
                    .map_or_else(|| c.t(Tr::GeneralOff).to_string(), |s| dito_platform::keymap::label(&s, c.lang));
                ("command-shortcut-field", label)
            }
        };
        div()
            .id(id)
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
            .on_click(cx.listener(move |this, _, _, _| this.st.hotkeys.begin_capture(target)))
            .child(text(label, 15., FontWeight::BOLD, p.text).line_height(relative(1.)))
            .into_any_element()
    }
}
