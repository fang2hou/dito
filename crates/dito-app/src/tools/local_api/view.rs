//! The page body: the head with the service switch, the API and MCP tabs, the connection card and the lists.

use std::rc::Rc;
use std::time::Duration;

use dito_core::i18n::Tr;
use dito_core::local_api::{CheckFailure, Status};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::spec::{self, Item, Tab};
use super::{Check, Dialog, LocalApiPage, SPEC_COPIED_MS};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, CircleSize, MethodSize, cbk, cbk_idx, icon, text};

const PAGE_PAD_X: f32 = 56.;
const PAGE_PAD_TOP: f32 = 40.;
const PAGE_PAD_BOTTOM: f32 = 40.;
const PAGE_GAP: f32 = 16.;
const ROW_PAD_X: f32 = 20.;
const ROW_GAP: f32 = 12.;
const LABEL_W: f32 = 132.;
const KIND_SLOT_W: f32 = 64.;
const PATH_W: f32 = 250.;

impl LocalApiPage {
    pub(super) fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = self.ctx(cx);
        let p = c.p;
        let cfg = self.st.cfg.get();
        let enabled = cfg.local_api.enabled;
        let status = self.st.local_api.status().borrow().clone();
        let page = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .px(px(PAGE_PAD_X))
            .pt(px(PAGE_PAD_TOP))
            .pb(px(PAGE_PAD_BOTTOM))
            .gap(px(PAGE_GAP))
            .child(self.head(&c, enabled, cx));
        let body = match (&status, enabled) {
            (_, false) => off(&c),
            (Status::Failed { .. }, true) => {
                div().w_full().child(ui::banner(&p, c.t(Tr::LocalApiPortUnavailable), None))
            }
            _ => self.running(&c, cx),
        };
        page.child(body).children(self.dialog_layer(&c, window, cx))
    }

    fn head(&self, c: &Ctx, enabled: bool, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let st = self.st.clone();
        let service = ui::switch(
            "local-api-service",
            &p,
            enabled,
            Rc::new(move |on, _, _| {
                crate::services::save_config(&st, move |cfg| cfg.local_api.enabled = on);
            }),
        )
        .role(Role::Switch)
        .aria_label(c.t(Tr::LocalApiServiceSwitch));
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(ui::page_title(&p, c.t(Tr::NavLocalApi)))
            .child(div().flex().items_center().gap(px(16.)).child(service).child(self.gear(c, cx)))
    }

    fn gear(&self, c: &Ctx, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = ElementId::from("local-api-settings");
        let label = c.t(Tr::NavSettings);
        let open = self.dialog == Some(Dialog::Settings);
        if !open {
            ui::tip_label(id.clone(), label.clone());
        }
        let button = ui::icon_button(
            id.clone(),
            &c.p,
            Ic::Settings,
            open,
            cbk(cx, |page: &mut Self, window, cx| page.open_dialog(Dialog::Settings, window, cx)),
        )
        .role(Role::Button)
        .aria_label(label);
        if open { button } else { button.child(ui::tip_above(&c.p, &id)) }
    }

    fn running(&mut self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let cfg = self.st.cfg.get();
        let facts = self.facts();
        let selected = if self.tab == Tab::Api { 0 } else { 1 };
        let tabs = ui::seg(
            "local-api-tabs",
            &p,
            &[c.t(Tr::LocalApiTabApi), c.t(Tr::LocalApiTabMcp)],
            selected,
            cbk_idx(cx, |page: &mut Self, i, _, cx| page.select_tab(if i == 0 { Tab::Api } else { Tab::Mcp }, cx)),
        );
        let tab_row =
            div().w_full().flex().items_center().justify_between().child(tabs).child(self.check_button(c, cx));
        let token = self.st.local_api.token();
        let mut body = div().w_full().flex().flex_col().gap(px(PAGE_GAP)).child(tab_row);
        let rows = match self.tab {
            Tab::Api => {
                let auth = auth_row(c, &p, cfg.local_api.api_any_token, &token, "local-api-copy-token");
                let models = model_row(c, &p, &facts);
                body = body.child(
                    card(&p)
                        .child(url_row(
                            c,
                            &p,
                            Tr::LocalApiBaseUrl,
                            format!("{}/v1", facts.base_url),
                            "local-api-copy-url",
                            false,
                        ))
                        .child(auth)
                        .child(models),
                );
                self.item_section(c, cx, Tr::LocalApiEndpoints, Item::API)
            }
            Tab::Mcp => {
                let auth = auth_row_for(
                    c,
                    &p,
                    Tr::LocalApiMcpAuth,
                    cfg.local_api.mcp_any_token,
                    &token,
                    "local-api-copy-mcp-token",
                    true,
                );
                body = body
                    .child(
                        card(&p)
                            .child(url_row(
                                c,
                                &p,
                                Tr::LocalApiMcpUrl,
                                format!("{}/mcp", facts.base_url),
                                "local-api-copy-mcp-url",
                                false,
                            ))
                            .child(auth),
                    )
                    .child(ui::code_block(
                        "local-api-mcp-config",
                        &p,
                        c.t(Tr::LocalApiMcpExample),
                        spec::mcp_client_config(&facts.base_url),
                    ));
                self.item_section(c, cx, Tr::LocalApiMcpItems, Item::MCP)
            }
        };
        let scroll =
            div().id("local-api-scroll").w_full().flex_1().min_h(px(0.)).overflow_y_scroll().child(body.child(rows));
        div().w_full().flex_1().min_h(px(0.)).flex().flex_col().child(scroll)
    }

    fn check_button(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let id = "local-api-check";
        match self.check {
            Check::Idle => ui::button(
                id,
                &p,
                &c.t(Tr::LocalApiCheck),
                BtnKind::Secondary,
                Some(Ic::Activity),
                cbk(cx, |page: &mut Self, _, cx| page.run_check(cx)),
            )
            .into_any_element(),
            Check::Running => ui::disabled_button(
                id,
                &p,
                &c.t(Tr::LocalApiChecking),
                BtnKind::Secondary,
                ui::BtnSize::Regular,
                Some(Ic::Activity),
                None,
            )
            .into_any_element(),
            Check::Done(Ok(())) => ui::button(
                id,
                &p,
                &c.t(Tr::LocalApiCheckOk),
                BtnKind::Secondary,
                Some(Ic::Check),
                cbk(cx, |page: &mut Self, _, cx| page.run_check(cx)),
            )
            .into_any_element(),
            Check::Done(Err(failure)) => {
                let label = match failure {
                    CheckFailure::Unauthorized => Tr::LocalApiCheckUnauthorized,
                    CheckFailure::Unreachable | CheckFailure::Other(_) => Tr::LocalApiCheckUnreachable,
                };
                ui::button(
                    id,
                    &p,
                    &c.t(label),
                    BtnKind::Danger,
                    Some(Ic::Alert),
                    cbk(cx, |page: &mut Self, _, cx| page.run_check(cx)),
                )
                .into_any_element()
            }
        }
    }

    /// A section head with the copy specification button, over the list of items that open their reference.
    fn item_section(&self, c: &Ctx, cx: &mut Context<Self>, title: Tr, items: [Item; 3]) -> Div {
        let p = c.p;
        let copied = self.spec_copied.is_some_and(|at| at.elapsed() < Duration::from_millis(SPEC_COPIED_MS));
        let copy = ui::button(
            "local-api-copy-spec",
            &p,
            &c.t(Tr::LocalApiCopySpec),
            BtnKind::Secondary,
            Some(if copied { Ic::Check } else { Ic::Copy }),
            cbk(cx, |page: &mut Self, _, cx| page.copy_spec(cx)),
        );
        let head = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(text(c.t(title), 15., FontWeight::BOLD, p.text))
            .child(copy);
        let last = items.len() - 1;
        let rows: Vec<_> = items.iter().enumerate().map(|(i, item)| self.item_row(c, cx, *item, i == last)).collect();
        div().w_full().flex().flex_col().gap(px(PAGE_GAP)).child(head).child(card(&p).children(rows))
    }

    fn item_row(&self, c: &Ctx, cx: &mut Context<Self>, item: Item, last: bool) -> Stateful<Div> {
        let p = c.p;
        let id = ElementId::from((ElementId::from("local-api-item"), item.path()));
        let label = item.kind_label().map_or_else(
            || SharedString::from(if item.kind() == ui::MethodKind::Get { "GET" } else { "POST" }),
            |tr| c.t(tr),
        );
        let mut row = div()
            .id(id.clone())
            .w_full()
            .h(px(46.))
            .px(px(ROW_PAD_X))
            .relative()
            .flex()
            .items_center()
            .gap(px(ROW_GAP))
            .cursor_pointer()
            .on_hover(ui::hover_tracker(id.clone()))
            .on_click(ui::click_cb(cbk(cx, move |page: &mut Self, window, cx| {
                page.open_dialog(Dialog::Reference(item), window, cx)
            })))
            .child(ui::hover_fill(&id, Some(0.), p.seg))
            .child(div().w(px(KIND_SLOT_W)).flex_none().flex().child(ui::method_label(
                &p,
                item.kind(),
                label,
                MethodSize::Row,
            )))
            .child(mono(item.path(), p.text).w(px(PATH_W)).flex_none())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(text(c.t(item.summary()), 13., FontWeight::NORMAL, p.muted).line_height(px(20.))),
            )
            .child(icon(Ic::ArrowRight, 16., p.text));
        if !last {
            row = row.border_b_1().border_color(p.border);
        }
        row
    }
}

/// The page while the service is off: one title and one line on how to turn it on.
fn off(c: &Ctx) -> Div {
    let p = c.p;
    div()
        .w_full()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .child(text(c.t(Tr::LocalApiOffTitle), 17., FontWeight::BOLD, p.text))
        .child(text(c.t(Tr::LocalApiOffHint), 13., FontWeight::NORMAL, p.muted).line_height(px(20.)))
}

fn card(p: &Palette) -> Div {
    div().w_full().flex().flex_col().overflow_hidden().rounded(px(16.)).bg(p.card).border_1().border_color(p.border)
}

fn mono(value: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .font_family("Menlo")
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(color)
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .child(value.into())
}

fn copy_to_clipboard(id: &'static str, p: &Palette, value: String) -> Stateful<Div> {
    ui::ghost_copy_button(
        id,
        p,
        CircleSize::Compact,
        Rc::new(move |_, cx: &mut App| {
            cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
            true
        }),
    )
}

fn detail_row(
    p: &Palette,
    label: SharedString,
    value: AnyElement,
    trailing: Option<Stateful<Div>>,
    height: f32,
    last: bool,
) -> Div {
    let mut row = div()
        .w_full()
        .h(px(height))
        .px(px(ROW_PAD_X))
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .child(
            div().w(px(LABEL_W)).flex_none().child(text(label, 13., FontWeight::NORMAL, p.muted).line_height(px(20.))),
        )
        .child(div().flex_1().min_w(px(0.)).child(value))
        .children(trailing);
    if !last {
        row = row.border_b_1().border_color(p.border);
    }
    row
}

fn url_row(c: &Ctx, p: &Palette, label: Tr, url: String, copy_id: &'static str, last: bool) -> Div {
    detail_row(
        p,
        c.t(label),
        mono(url.clone(), p.text).into_any_element(),
        Some(copy_to_clipboard(copy_id, p, url)),
        44.,
        last,
    )
}

fn auth_row(c: &Ctx, p: &Palette, any_token: bool, token: &str, copy_id: &'static str) -> Div {
    auth_row_for(c, p, Tr::LocalApiAccessToken, any_token, token, copy_id, false)
}

/// The token with its Copy, or the line that says no token is needed.
fn auth_row_for(
    c: &Ctx,
    p: &Palette,
    label: Tr,
    any_token: bool,
    token: &str,
    copy_id: &'static str,
    last: bool,
) -> Div {
    if any_token {
        let value = text(c.t(Tr::LocalApiAnyToken), 13., FontWeight::NORMAL, p.text).line_height(px(20.));
        return detail_row(p, c.t(label), value.into_any_element(), None, 44., last);
    }
    detail_row(
        p,
        c.t(label),
        mono(mask_token(token), p.text).into_any_element(),
        Some(copy_to_clipboard(copy_id, p, token.to_owned())),
        44.,
        false,
    )
}

fn model_row(c: &Ctx, p: &Palette, facts: &spec::Facts) -> Div {
    let chips = div()
        .flex()
        .flex_wrap()
        .gap(px(12.))
        .child(model_chip(c, p, Tr::LocalApiRoleText, &facts.text_model, "local-api-copy-text-model"))
        .child(model_chip(c, p, Tr::LocalApiRoleSpeech, &facts.speech_model, "local-api-copy-speech-model"));
    detail_row(p, c.t(Tr::LocalApiModelIds), chips.into_any_element(), None, 48., true)
}

fn model_chip(c: &Ctx, p: &Palette, role: Tr, model: &str, copy_id: &'static str) -> Div {
    div()
        .h(px(28.))
        .pl(px(10.))
        .pr(px(2.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .rounded(px(8.))
        .bg(p.sub)
        .child(text(c.t(role), 12., FontWeight::NORMAL, p.muted).line_height(px(18.)))
        .child(mono(model.to_owned(), p.text))
        .child(copy_to_clipboard(copy_id, p, model.to_owned()))
}

/// Shows enough of the token to recognise it without handing it to anyone looking at the screen.
fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 16 {
        return "•".repeat(chars.len());
    }
    let head: String = chars[..9].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}{}{tail}", "•".repeat(16))
}

#[cfg(test)]
mod tests {
    use super::mask_token;

    #[test]
    fn a_token_is_shown_by_its_ends_and_a_short_one_not_at_all() {
        assert_eq!(mask_token("dito_7f3a9c2e5b81d04f6a2e9b7c91e"), "dito_7f3a••••••••••••••••c91e");
        assert_eq!(mask_token("short"), "•••••");
        assert_eq!(mask_token(""), "");
    }
}
