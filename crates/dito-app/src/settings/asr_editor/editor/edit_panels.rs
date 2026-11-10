use dito_core::i18n::Tr;
use dito_core::provider::configured_asr::AsrProfiles;
use gpui_kit::base::input::{Redo, Undo};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{AsrEditor, ISO_CODES, LOCALE_CODES};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, Editor, cbk, cbk_idx, text};

impl AsrEditor {
    pub(super) fn inline(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        self.style_source(&p, cx);
        if self.tested_language.is_some_and(|language| language != self.st.cfg.get().general.preferred_language) {
            self.test = ui::TestState::Idle;
            self.tested_language = None;
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(self.templates(&c, cx))
            .child(ui::form_field(&p, c.t(Tr::ModelsApiKey), ui::text_input(&p, &self.key, None).form()))
            .child(self.toolbar(&c, cx))
            .child(self.code_box(&c, cx))
            .child(self.validation_card(&c))
            .child(self.connection_footer(&c, cx))
    }

    fn templates(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let names = AsrProfiles::NAMES.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>();
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(text(c.t(Tr::AsrEditorTemplate), 13., FontWeight::MEDIUM, c.p.muted))
            .child(ui::select(
                "asr-template",
                &c.p,
                &names,
                self.selected,
                self.template_open,
                Some(200.),
                cbk(cx, |this: &mut Self, _, cx| {
                    this.template_open = !this.template_open;
                    cx.notify();
                }),
                cbk_idx(cx, |this: &mut Self, i, w, cx| this.switch(i, w, cx)),
            ))
    }

    fn toolbar(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let actions = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(editor_icon(
                "asr-undo",
                p,
                c.t(Tr::AsrEditorUndo),
                Ic::Undo,
                cbk(cx, |this: &mut Self, w, cx| {
                    this.source.read(cx).focus_handle(cx).focus(w, cx);
                    w.dispatch_action(Box::new(Undo), cx);
                }),
            ))
            .child(editor_icon(
                "asr-redo",
                p,
                c.t(Tr::AsrEditorRedo),
                Ic::Redo,
                cbk(cx, |this: &mut Self, w, cx| {
                    this.source.read(cx).focus_handle(cx).focus(w, cx);
                    w.dispatch_action(Box::new(Redo), cx);
                }),
            ))
            .child(editor_icon(
                "copy-config",
                p,
                c.t(Tr::AsrEditorCopy),
                Ic::Copy,
                cbk(cx, |this: &mut Self, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(this.source.read(cx).value().to_string()));
                }),
            ));
        div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(text(c.t(Tr::AsrEditorLanguage), 13., FontWeight::NORMAL, p.muted))
                    .child(ui::button(
                        "map-iso",
                        p,
                        "ja / en / zh",
                        BtnKind::Secondary,
                        None,
                        cbk(cx, |this: &mut Self, w, cx| this.map_language_codes(ISO_CODES, w, cx)),
                    ))
                    .child(ui::button(
                        "map-locale",
                        p,
                        "ja-JP / en-US / zh-CN",
                        BtnKind::Secondary,
                        None,
                        cbk(cx, |this: &mut Self, w, cx| this.map_language_codes(LOCALE_CODES, w, cx)),
                    )),
            )
            .child(div().flex_1())
            .child(actions)
    }

    fn code_box(&self, c: &Ctx, cx: &App) -> Stateful<Div> {
        let p = &c.p;
        let source = self.source.read(cx).value().to_string();
        div()
            .w_full()
            .id("asr-source-field")
            .on_mouse_down_out(ui::blur_on_outside(self.source.read(cx).focus_handle(cx)))
            .on_mouse_down(MouseButton::Left, ui::focus_on_click(self.source.read(cx).focus_handle(cx)))
            .h(px(360.))
            .flex_none()
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(p.border)
            .bg(p.sub)
            .overflow_hidden()
            .child(
                div().flex_1().min_h(px(0.)).child(
                    Editor::new(&self.source)
                        .appearance(false)
                        .bordered(false)
                        .h_full()
                        .font_family("Menlo")
                        .text_size(px(12.))
                        .aria_label(c.t(Tr::AsrEditorTitle)),
                ),
            )
            .child(
                div()
                    .h(px(26.))
                    .flex_none()
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(p.border)
                    .child(text(
                        c.t(Tr::FmtAsrLineCount).replace("{count}", &source.lines().count().max(1).to_string()),
                        12.,
                        FontWeight::NORMAL,
                        p.muted,
                    )),
            )
    }

    fn validation_card(&self, c: &Ctx) -> Div {
        let p = &c.p;
        let status = if self.pending {
            c.t(Tr::AsrEditorPending).to_string()
        } else {
            self.error.clone().unwrap_or_else(|| c.t(Tr::AsrEditorValid).to_string())
        };
        let dot = if self.error.is_some() {
            p.danger
        } else if self.pending {
            p.muted
        } else {
            p.success
        };
        div()
            .w_full()
            .p(px(14.))
            .rounded(px(12.))
            .bg(p.sub)
            .border_1()
            .border_color(p.border)
            .flex()
            .items_start()
            .gap(px(8.))
            .child(div().size(px(7.)).mt(px(6.)).flex_none().rounded_full().bg(dot))
            .child(text(status, 13., FontWeight::NORMAL, p.text).line_height(px(20.)))
    }

    fn connection_footer(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let mut status = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(crate::settings::models::test_status(c, &self.test));
        if let ui::TestState::Failed(message) = &self.test {
            status = status.child(text(ui::error_line(c.lang, message), 12., FontWeight::NORMAL, p.text));
        }
        let ready = self.can_test(cx);
        div()
            .w_full()
            .pt(px(16.))
            .border_t_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .gap(px(16.))
            .child(status)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        ui::button(
                            "asr-test",
                            p,
                            &c.t(Tr::ModelsTestConnection),
                            BtnKind::Subtle,
                            None,
                            cbk(cx, |this: &mut Self, _, cx| this.run_test(cx)),
                        )
                        .opacity(if ready { 1. } else { 0.45 }),
                    )
                    .child(ui::button(
                        "asr-help",
                        p,
                        &c.t(Tr::AsrEditorHelp),
                        BtnKind::Secondary,
                        Some(Ic::BookOpen),
                        cbk(cx, |this: &mut Self, w, cx| this.open_help(w, cx)),
                    )),
            )
    }
}

fn editor_icon(id: &'static str, p: &Palette, label: SharedString, icon: Ic, action: ui::Cb) -> Stateful<Div> {
    let key = ElementId::from(id);
    ui::tip_label(key.clone(), label);
    ui::icon_button(key.clone(), p, icon, false, action).child(ui::tip_above(p, &key))
}
