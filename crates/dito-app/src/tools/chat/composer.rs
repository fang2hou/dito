//! The message field with its pending attachments, the two add actions and Send or Stop (`design:tools/chat`,
//! 23.05.1 and 23.05.5).

use dito_core::chat::{THINKING_OFF, THINKING_ON, thinking_choices};
use dito_core::i18n::{Tr, fmt};
use dito_core::provider::ThinkingCapabilities;
use dito_core::store::Attachment;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::files::{FileKind, file_kind, size_label};
use super::{ChatPage, picker};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, CircleAppearance, CircleSize, TextAreaStyle};

const CHIP_NAME_MAX: f32 = 150.;

pub(super) fn view(page: &ChatPage, c: &Ctx, window: &mut Window, cx: &mut Context<ChatPage>) -> Div {
    let p = c.p;
    let _ = window;
    let mut shell = div()
        .relative()
        .w_full()
        .flex_none()
        .px(px(14.))
        .pt(px(12.))
        .pb(px(8.))
        .flex()
        .flex_col()
        .rounded(px(16.))
        .bg(p.card)
        .border_1()
        .border_color(p.border_strong);
    if !page.pending.is_empty() {
        let mut chips = div().w_full().pb(px(10.)).flex().flex_wrap().gap(px(8.));
        for (i, attachment) in page.pending.iter().enumerate() {
            chips = chips.child(chip(i, attachment, c, cx));
        }
        shell = shell.child(chips);
    }
    shell = shell
        .child(ui::text_area(
            &page.composer,
            TextAreaStyle {
                bg: ui::transparent(),
                edge: None,
                radius: 0.,
                text_size: 14.,
                line_height: 22.,
                pad: [0., 0., 0., 0.],
                fill: false,
                reserve_bottom: 0.,
            },
        ))
        .child(div().h(px(12.)).flex_none())
        .child(actions(page, c, cx));
    if let Some(open) = &page.picker {
        shell = shell.child(
            div().absolute().left(px(14.)).bottom(px(44.)).child(deferred(picker::view(open, c, cx)).with_priority(1)),
        );
    }
    shell
}

fn actions(page: &ChatPage, c: &Ctx, cx: &mut Context<ChatPage>) -> Div {
    let p = c.p;
    let left = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(pill(
            "chat-add-file",
            Ic::FileText,
            c.t(Tr::ChatAddFile),
            &p,
            ui::cbk(cx, |page, window, cx| page.add_files(window, cx)),
        ))
        .child(pill(
            "chat-add-meeting",
            Ic::AudioLines,
            c.t(Tr::ChatAddMeeting),
            &p,
            ui::cbk(cx, |page, window, cx| page.open_picker(window, cx)),
        ))
        .children(thinking(page, c, cx));
    div().h(px(32.)).w_full().flex().items_center().justify_between().child(left).child(send_or_stop(page, c, cx))
}

/// The thinking pill with its menu above it, for a model that declares a thinking control (`design:tools/chat`,
/// 23.05.7). A level shows the name its model gives it, in every display language.
fn thinking(page: &ChatPage, c: &Ctx, cx: &mut Context<ChatPage>) -> Option<Div> {
    let p = c.p;
    let facts = page.model_facts();
    let capabilities = facts.thinking?;
    let current = page.thinking_choice(&facts);
    let title = c.t(Tr::ChatThinking);
    let label = match current {
        Some(choice) => SharedString::from(format!("{title} {}", choice_name(choice, &capabilities, c))),
        None => title,
    };
    let button = pill("chat-thinking", Ic::Zap, label, &p, ui::cbk(cx, |page, _, cx| page.toggle_thinking_menu(cx)));
    let mut wrap = div().relative().child(button);
    if page.thinking_menu {
        let close = ui::cbk(cx, |page, _, cx| page.close_thinking_menu(cx));
        let mut panel = ui::menu_panel(ElementId::from("chat-thinking-menu"), &p, close).w(px(200.));
        for (i, choice) in thinking_choices(&capabilities).into_iter().enumerate() {
            let row = ui::menu_row(ElementId::from(("chat-thinking-choice", i)), &p)
                .justify_between()
                .on_click(ui::click_cb(ui::cbk(cx, move |page, _, cx| page.set_thinking(choice, cx))))
                .child(
                    ui::text(choice_name(choice, &capabilities, c), 13., FontWeight::NORMAL, p.text)
                        .line_height(relative(1.)),
                )
                .children((current == Some(choice)).then(|| ui::icon(Ic::Check, 14., p.accent_text)));
            panel = panel.child(row);
        }
        wrap = wrap.child(div().absolute().left_0().bottom(px(34.)).child(deferred(panel).with_priority(1)));
    }
    Some(wrap)
}

fn choice_name(choice: &str, capabilities: &ThinkingCapabilities, c: &Ctx) -> SharedString {
    match choice {
        THINKING_OFF => c.t(Tr::ChatThinkingOff),
        THINKING_ON => c.t(Tr::ChatThinkingOn),
        key => {
            capabilities.levels.iter().find(|level| level.key == key).map_or(key, |level| level.name).to_owned().into()
        }
    }
}

/// A 28 pt pill on the sub surface, glyph and label muted.
fn pill(id: &'static str, glyph: Ic, label: SharedString, p: &Palette, on_click: ui::Cb) -> Stateful<Div> {
    let id = ElementId::from(id);
    div()
        .id(id.clone())
        .relative()
        .h(px(28.))
        .pl(px(10.))
        .pr(px(12.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded_full()
        .bg(p.sub)
        .cursor_pointer()
        .on_hover(ui::hover_tracker(id.clone()))
        .on_click(ui::click_cb(on_click))
        .child(ui::hover_fill(&id, None, p.seg))
        .child(ui::icon(glyph, 14., p.muted))
        .child(ui::text(label, 12., FontWeight::NORMAL, p.muted).line_height(px(18.)))
}

fn send_or_stop(page: &ChatPage, c: &Ctx, cx: &mut Context<ChatPage>) -> impl IntoElement {
    let p = c.p;
    if page.generating() {
        let label = c.t(Tr::ChatStop);
        return ui::circle_control_with_glyph(
            "chat-stop",
            Ic::Stop,
            CircleSize::Small,
            14.,
            CircleAppearance {
                background: Some(p.button_danger_bg),
                foreground: p.button_danger_text,
                border: Some(p.button_danger_border),
                hover: Some(p.button_danger_hover),
            },
        )
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| {
            page.stop();
            cx.notify();
        })))
        .into_any_element();
    }
    let empty = page.composer.read(cx).value().trim().is_empty() && page.pending.is_empty();
    let label = c.t(Tr::ChatSend);
    let appearance = CircleAppearance {
        background: Some(p.input_bg),
        foreground: p.text,
        border: Some(p.border_strong),
        hover: (!empty).then_some(p.seg),
    };
    let button = ui::circle_control_with_glyph("chat-send", Ic::Send, CircleSize::Small, 16., appearance)
        .role(Role::Button)
        .aria_label(label);
    if empty {
        return button.opacity(0.45).into_any_element();
    }
    button
        .cursor_pointer()
        .on_click(ui::click_cb(ui::cbk(cx, |page, window, cx| page.send(window, cx))))
        .into_any_element()
}

/// A 44 pt chip: a 32 pt tile, the name and what it is. Hovering shows Remove.
fn chip(index: usize, attachment: &Attachment, c: &Ctx, cx: &mut Context<ChatPage>) -> impl IntoElement {
    let p = c.p;
    let id = ElementId::from(("chat-chip", index));
    let (tile_fill, tile_glyph, glyph_color) = match attachment {
        Attachment::File { .. } => (p.card, Ic::FileText, p.muted),
        Attachment::Meeting { .. } => (p.accent_soft, Ic::AudioLines, p.accent_text),
    };
    let detail = match attachment {
        Attachment::File { name, bytes, .. } => {
            let kind = match file_kind(name) {
                FileKind::Markdown => Tr::ChatKindMarkdown,
                FileKind::Text => Tr::ChatKindText,
                FileKind::Code => Tr::ChatKindCode,
            };
            fmt::chat_file_detail(c.lang, c.lang.tr(kind), &size_label(*bytes))
        }
        Attachment::Meeting { duration_ms, .. } => {
            fmt::chat_meeting_detail(c.lang, &crate::timefmt::clock(duration_ms.unwrap_or(0)))
        }
    };
    let hovered = ui::is_hovered(&id);
    let remove = hovered.then(|| {
        let label = c.t(Tr::ChatRemoveAttachment);
        ui::circle_control(
            ElementId::from(("chat-chip-remove", index)),
            Ic::X,
            CircleSize::Compact,
            CircleAppearance {
                background: Some(p.card),
                foreground: p.muted,
                border: Some(p.border),
                hover: Some(p.seg),
            },
        )
        .absolute()
        .top(px(-6.))
        .right(px(-6.))
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(ui::click_cb(ui::cbk(cx, move |page, _, cx| page.remove_pending(index, cx))))
    });
    div()
        .id(id.clone())
        .relative()
        .h(px(44.))
        .pl(px(6.))
        .pr(px(14.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(12.))
        .bg(p.sub)
        .border_1()
        .border_color(p.border)
        .on_hover(ui::hover_tracker(id))
        .child(
            div()
                .size(px(32.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(tile_fill)
                .child(ui::icon(tile_glyph, 16., glyph_color)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    ui::text(attachment.name().to_owned(), 13., FontWeight::MEDIUM, p.text)
                        .line_height(px(18.))
                        .max_w(px(CHIP_NAME_MAX))
                        .truncate(),
                )
                .child(ui::text(detail, 11., FontWeight::NORMAL, p.muted).line_height(px(14.))),
        )
        .children(remove)
}
