//! The two dialogs of the page: the service settings from the gear (`design:tools/local-api`, 23.06.3) and the
//! reference of one endpoint, tool or resource (23.06.4).

use std::rc::Rc;

use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::spec::{self, Item, Reference};
use super::{Dialog, LocalApiPage};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, CircleAppearance, CircleSize, MethodSize, ToolDialogHeader, cbk, click_cb, text};

const CONTENT_GAP: f32 = 18.;
const REFERENCE_GAP: f32 = 14.;

impl LocalApiPage {
    /// The open dialog over the whole window, sidebar included, or nothing.
    pub(super) fn dialog_layer(&self, c: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialog?;
        let close = cbk(cx, |page: &mut Self, _, cx| page.close_dialog(cx));
        let element = match dialog {
            Dialog::Settings => ui::tool_settings_dialog(
                window,
                &c.p,
                "local-api-settings-dialog",
                &self.focus,
                c.t(Tr::LocalApiSettingsTitle),
                self.settings_content(c, cx).into_any_element(),
                close,
            ),
            Dialog::Reference(item) => self.reference_dialog(c, item, window, cx, close),
        };
        let size = window.viewport_size();
        Some(
            deferred(
                anchored()
                    .position(point(px(0.), px(0.)))
                    .child(div().relative().w(size.width).h(size.height).child(element)),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    fn settings_content(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let cfg = self.st.cfg.get();
        let port = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(16.))
            .child(titled(c, Tr::LocalApiPort, Tr::LocalApiPortHint))
            .child(
                div()
                    .w(px(96.))
                    .flex_none()
                    .child(ui::text_input(&p, &self.port, None).invalid(self.port_invalid))
                    .font_family("Menlo")
                    .text_size(px(12.)),
            );
        let token_field = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().min_w(px(0.)).font_family("Menlo").text_size(px(12.)).child(ui::text_input(
                &p,
                &self.token,
                None,
            )))
            .child(self.generate_button(c, cx))
            .child(self.copy_token_button(c, cx));
        let token = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(titled(c, Tr::LocalApiAccessToken, Tr::LocalApiTokenHint))
            .child(token_field);
        let st = self.st.clone();
        let api = self.switch_row(
            c,
            "local-api-api-any-token",
            Tr::LocalApiApiAnyToken,
            Tr::LocalApiApiAnyTokenHint,
            cfg.local_api.api_any_token,
            Rc::new({
                let st = st.clone();
                move |on, _, _| crate::services::save_config(&st, move |cfg| cfg.local_api.api_any_token = on)
            }),
        );
        let mcp = self.switch_row(
            c,
            "local-api-mcp-any-token",
            Tr::LocalApiMcpAnyToken,
            Tr::LocalApiMcpAnyTokenHint,
            cfg.local_api.mcp_any_token,
            Rc::new(move |on, _, _| crate::services::save_config(&st, move |cfg| cfg.local_api.mcp_any_token = on)),
        );
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(CONTENT_GAP))
            .child(port)
            .child(token)
            .child(div().w_full().h(px(1.)).bg(p.border))
            .child(api)
            .child(mcp)
    }

    fn switch_row(&self, c: &Ctx, id: &'static str, title: Tr, hint: Tr, on: bool, toggle: ui::ToggleCb) -> Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(16.))
            .child(titled(c, title, hint))
            .child(div().flex_none().child(ui::switch(id, &c.p, on, toggle)))
    }

    fn generate_button(&self, c: &Ctx, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = c.p;
        let id = ElementId::from("local-api-generate-token");
        let label = c.t(Tr::LocalApiGenerateToken);
        ui::tip_label(id.clone(), label.clone());
        ui::circle_control(
            id.clone(),
            Ic::Refresh,
            CircleSize::Small,
            CircleAppearance { background: None, foreground: p.muted, border: None, hover: Some(p.seg) },
        )
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(click_cb(cbk(cx, |page: &mut Self, window, cx| {
            match page.st.local_api.regenerate_token() {
                Ok(token) => page.token.update(cx, |input, cx| input.set_value(token, window, cx)),
                Err(e) => tracing::warn!("could not save the Local API token: {e}"),
            }
            cx.notify();
        })))
        .child(ui::tip_above(&p, &id))
    }

    fn copy_token_button(&self, c: &Ctx, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = c.p;
        let id = ElementId::from("local-api-copy-token-field");
        let label = c.t(Tr::LocalApiCopyToken);
        ui::tip_label(id.clone(), label);
        let token = cbk_copy_token(cx);
        ui::ghost_copy_button(id.clone(), &p, CircleSize::Small, token).child(ui::tip_above(&p, &id))
    }

    fn reference_dialog(
        &self,
        c: &Ctx,
        item: Item,
        window: &Window,
        cx: &mut Context<Self>,
        close: ui::Cb,
    ) -> AnyElement {
        let p = c.p;
        let facts = self.facts();
        let t = |tr| c.t(tr).to_string();
        let reference = spec::reference(item, &facts, &t);
        let markdown = spec::markdown(&reference, &t);
        let copy = ui::ghost_copy_button(
            "local-api-copy-reference",
            &p,
            CircleSize::Small,
            Rc::new(move |_, cx: &mut App| {
                cx.write_to_clipboard(ClipboardItem::new_string(markdown.clone()));
                true
            }),
        );
        let leading = ui::method_label(&p, item.kind(), reference.kind.clone(), MethodSize::Title).into_any_element();
        let header = ToolDialogHeader {
            width: 620.,
            title: SharedString::from(item.path()),
            leading: Some(leading),
            actions: Some(copy.into_any_element()),
        };
        let _ = cx;
        ui::tool_settings_dialog_with(
            window,
            &p,
            "local-api-reference-dialog",
            &self.focus,
            header,
            reference_content(c, &p, &reference).into_any_element(),
            close,
        )
    }
}

fn cbk_copy_token(cx: &mut Context<LocalApiPage>) -> ui::CopyCb {
    ui::cbk_copy(cx, |page: &mut LocalApiPage, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(page.st.local_api.token()));
        true
    })
}

/// A 14 pt title over a 13 pt muted line, the way every row of the tool settings dialog is labelled.
fn titled(c: &Ctx, title: Tr, hint: Tr) -> Div {
    let p = c.p;
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(text(c.t(title), 14., FontWeight::MEDIUM, p.text).line_height(px(20.)))
        .child(text(c.t(hint), 13., FontWeight::NORMAL, p.muted).line_height(px(18.)))
}

fn reference_content(c: &Ctx, p: &Palette, reference: &Reference) -> Div {
    let item = reference.item;
    let mut meta_rows = vec![meta_row(p, c.t(Tr::LocalApiRefAuth), reference.auth.clone(), reference.model.is_some())];
    if let Some(model) = &reference.model {
        meta_rows.push(meta_row(p, c.t(Tr::LocalApiRefModel), model.clone(), false));
    }
    let meta = div()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(12.))
        .border_1()
        .border_color(p.border)
        .children(meta_rows);
    let blocks = reference.blocks.iter().enumerate().map(|(i, block)| {
        ui::code_block(
            ElementId::from((
                ElementId::from(("local-api-ref-block", item as usize)),
                SharedString::from(format!("{i}")),
            )),
            p,
            block.label.clone(),
            block.body.clone(),
        )
    });
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(REFERENCE_GAP))
        .child(text(reference.description.clone(), 13., FontWeight::NORMAL, p.text).line_height(px(20.)))
        .child(meta)
        .children(blocks)
}

fn meta_row(p: &Palette, label: SharedString, value: String, border: bool) -> Div {
    let mut row = div()
        .w_full()
        .h(px(38.))
        .px(px(14.))
        .flex()
        .items_center()
        .gap(px(12.))
        .child(div().w(px(72.)).flex_none().child(text(label, 12., FontWeight::NORMAL, p.muted).line_height(px(18.))))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .font_family("Menlo")
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(p.text)
                .child(value),
        );
    if border {
        row = row.border_b_1().border_color(p.border);
    }
    row
}
