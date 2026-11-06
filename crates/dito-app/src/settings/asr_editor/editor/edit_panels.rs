use dito_core::i18n::Tr;
use dito_core::provider::configured_asr::AsrProfiles;
use gpui_kit::component::input::Editor;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{AsrEditor, Closed, ISO_CODES, LOCALE_CODES};
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, text};

impl AsrEditor {
    pub(super) fn edit_panels(&self, body: Div, c: &Ctx, cx: &mut Context<Self>) -> Div {
        body.child(self.templates(&c.p, cx))
            .child(self.toolbar(c, cx))
            .child(self.code_box(c, cx))
            .child(self.validation_card(c))
    }

    fn templates(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let mut templates = div().w_full().flex().gap(px(12.));
        for (i, name) in AsrProfiles::NAMES.iter().enumerate() {
            let selected = self.selected == i;
            templates = templates.child(
                div()
                    .id(("json-template", i))
                    .flex_1()
                    .h(px(44.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(if selected { p.accent } else { p.border_strong })
                    .bg(if selected { p.accent_soft } else { p.bg })
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| this.switch(i, window, cx)))
                    .child(text(*name, 14., FontWeight::MEDIUM, p.text)),
            );
        }
        templates
    }

    fn toolbar(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.))
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
            ))
            .child(div().flex_1())
            .child(ui::button(
                "copy-config",
                p,
                &c.t(Tr::AsrEditorCopy),
                BtnKind::Secondary,
                None,
                cbk(cx, |this: &mut Self, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(this.source.read(cx).value().to_string()))
                }),
            ))
    }

    fn code_box(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let source = self.source.read(cx).value().to_string();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(p.border_strong)
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
        ui::card(p)
            .flex_none()
            .p(px(16.))
            .gap(px(4.))
            .child(text(c.t(Tr::AsrEditorValidation), 13., FontWeight::MEDIUM, p.text))
            .child(
                div().flex().items_center().gap(px(8.)).child(div().size(px(7.)).rounded_full().bg(dot)).child(text(
                    status,
                    13.,
                    FontWeight::NORMAL,
                    p.text,
                )),
            )
    }

    pub(super) fn edit_footer(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let can_save = !self.pending && self.error.is_none();
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                ui::button(
                    "asr-help",
                    p,
                    &c.t(Tr::AsrEditorHelp),
                    BtnKind::Secondary,
                    None,
                    cbk(cx, |this: &mut Self, _, cx| {
                        this.help = true;
                        cx.notify();
                    }),
                )
                .bg(p.accent_soft)
                .border_color(p.accent),
            )
            .child(div().flex_1())
            .child(ui::button(
                "asr-cancel",
                p,
                &c.t(Tr::AsrEditorCancel),
                BtnKind::Secondary,
                None,
                cbk(cx, |_: &mut Self, _, cx| cx.emit(Closed(false))),
            ))
            .child(
                ui::button(
                    "asr-save",
                    p,
                    &c.t(Tr::AsrEditorSave),
                    BtnKind::Primary,
                    None,
                    cbk(cx, move |this: &mut Self, _, cx| {
                        if can_save {
                            this.save(cx);
                        }
                    }),
                )
                .opacity(if can_save { 1. } else { 0.4 }),
            )
    }
}
