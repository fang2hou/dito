use dito_core::i18n::Tr;
use gpui_kit::base::input::{Redo, Undo};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{AsrEditor, ISO_CODES, LOCALE_CODES};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::settings::asr_editor::configuration_list;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, Editor, ErrorPlacement, FooterTrash, TestButton, cbk, text};

impl AsrEditor {
    pub(super) fn inline(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.sync(window, cx);
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        self.style_source(&p, cx);
        let language = self.st.cfg.get().general.preferred_language;
        if let Some(draft) = self.drafts.get_mut(&self.shown)
            && draft.tested_language.is_some_and(|tested| tested != language)
        {
            draft.test = ui::TestState::Idle;
            draft.tested_language = None;
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(self.tile_row(&c, cx))
            .child(self.name_and_key(&c))
            .child(self.toolbar(&c, cx))
            .child(self.code_box(&c, cx))
            .child(self.validation_card(&c))
            .child(self.connection_footer(&c, cx))
    }

    fn name_and_key(&self, c: &Ctx) -> Div {
        let p = &c.p;
        div()
            .w_full()
            .flex()
            .gap(px(16.))
            .child(
                ui::form_field(p, c.t(Tr::ModelsName), ui::text_input(p, &self.name, None).form())
                    .w(px(216.))
                    .flex_none(),
            )
            .child(
                ui::form_field(p, c.t(Tr::ModelsApiKey), ui::text_input(p, &self.key, None).form())
                    .flex_1()
                    .min_w(px(0.)),
            )
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
                    this.focus_source(w, cx);
                    w.dispatch_action(Box::new(Undo), cx);
                }),
            ))
            .child(editor_icon(
                "asr-redo",
                p,
                c.t(Tr::AsrEditorRedo),
                Ic::Redo,
                cbk(cx, |this: &mut Self, w, cx| {
                    this.focus_source(w, cx);
                    w.dispatch_action(Box::new(Redo), cx);
                }),
            ))
            .child(editor_icon(
                "copy-config",
                p,
                c.t(Tr::AsrEditorCopy),
                Ic::Copy,
                cbk(cx, |this: &mut Self, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(this.draft().source.read(cx).value().to_string()));
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
        let source = &self.draft().source;
        let text_now = source.read(cx).value().to_string();
        div()
            .w_full()
            .id("asr-source-field")
            .on_mouse_down_out(ui::blur_on_outside(source.read(cx).focus_handle(cx)))
            .on_mouse_down(MouseButton::Left, ui::focus_on_click(source.read(cx).focus_handle(cx)))
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
                    Editor::new(source)
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
                        c.t(Tr::FmtAsrLineCount).replace("{count}", &text_now.lines().count().max(1).to_string()),
                        12.,
                        FontWeight::NORMAL,
                        p.muted,
                    )),
            )
    }

    fn validation_card(&self, c: &Ctx) -> Div {
        let p = &c.p;
        let draft = self.draft();
        let status = if draft.pending {
            c.t(Tr::AsrEditorPending).to_string()
        } else {
            draft.error.clone().unwrap_or_else(|| c.t(Tr::AsrEditorValid).to_string())
        };
        let dot = if draft.error.is_some() {
            p.danger
        } else if draft.pending {
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
            .child(div().size(px(7.)).mt(px(7.)).flex_none().rounded_full().bg(dot))
            .child(text(status, 13., FontWeight::NORMAL, p.text).line_height(px(21.)))
    }

    /// The trash is disabled while the form shows the only configuration.
    fn connection_footer(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let help = ui::button(
            "asr-help",
            p,
            &c.t(Tr::AsrEditorHelp),
            BtnKind::Secondary,
            Some(Ic::BookOpen),
            cbk(cx, |this: &mut Self, w, cx| this.open_help(w, cx)),
        );
        ui::form_footer(
            p,
            ui::test_summary(p, c.lang, &self.draft().test, ErrorPlacement::Below),
            FooterTrash {
                id: "asr-delete",
                tip: Some(c.t(Tr::ModelsDelete)),
                on_click: (configuration_list::of(&self.st.cfg.get()).items.len() > 1)
                    .then(|| cbk(cx, |this: &mut Self, window, cx| this.ask_delete_configuration(window, cx))),
            },
            TestButton {
                id: "asr-test",
                label: c.t(Tr::ModelsTestConnection),
                width: None,
                on_click: self.can_test(&self.shown, cx).then(|| cbk(cx, |this: &mut Self, _, cx| this.run_test(cx))),
            },
            vec![help.into_any_element()],
        )
    }
}

fn editor_icon(id: &'static str, p: &Palette, label: SharedString, icon: Ic, action: ui::Cb) -> Stateful<Div> {
    let key = ElementId::from(id);
    ui::tip_label(key.clone(), label);
    ui::icon_button(key.clone(), p, icon, false, action).child(ui::tip_above(p, &key))
}
