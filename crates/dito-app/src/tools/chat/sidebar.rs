//! The session list: a trailing sidebar at the normal width, a drawer that opens on demand below it, and each
//! session's actions (`design:tools/chat`, 23.05.1 to 23.05.3).

use std::time::{Duration, Instant};

use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{ChatPage, DRAWER_MS, SIDEBAR_W};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, CircleAppearance, CircleSize, InputEvent, InputState};

const HEADER_H: f32 = 53.;
const ROW_H: f32 = 40.;

/// The open actions menu of one session; Delete asks once more before it acts.
pub(super) struct RowMenu {
    pub id: i64,
    pub armed: bool,
}

pub(super) struct Rename {
    pub id: i64,
    input: Entity<InputState>,
}

impl ChatPage {
    pub(super) fn start_rename(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.sessions.iter().find(|s| s.id == id).map(|s| s.title.clone()).unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title));
        cx.subscribe_in(&input, window, move |page: &mut Self, state, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { .. } | InputEvent::Blur if page.rename.as_ref().is_some_and(|r| r.id == id) => {
                let value = state.read(cx).value().to_string();
                page.rename_chat(id, &value, cx);
            }
            _ => {}
        })
        .detach();
        input.update(cx, |state, cx| state.focus(window, cx));
        self.menu = None;
        self.rename = Some(Rename { id, input });
        cx.notify();
    }

    fn toggle_drawer(&mut self, cx: &mut Context<Self>) {
        self.drawer = !self.drawer;
        self.menu = None;
        cx.notify();
    }
}

/// The compact toolbar's button that opens the drawer.
pub(super) fn history_button(c: &Ctx, cx: &mut Context<ChatPage>) -> impl IntoElement {
    let p = c.p;
    let id = ElementId::from("chat-history-open");
    div()
        .id(id.clone())
        .relative()
        .h(px(32.))
        .px(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .rounded_full()
        .bg(p.card)
        .border_1()
        .border_color(p.border_strong)
        .cursor_pointer()
        .on_hover(ui::hover_tracker(id.clone()))
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.toggle_drawer(cx))))
        .child(ui::hover_fill(&id, None, p.seg))
        .child(ui::text(c.t(Tr::ChatHistoryOpen), 12., FontWeight::NORMAL, p.text).line_height(px(18.)))
}

/// The drawer over the conversation with the scrim behind it, while it is open or still leaving.
pub(super) fn drawer(
    page: &mut ChatPage,
    c: &Ctx,
    window: &mut Window,
    cx: &mut Context<ChatPage>,
) -> Option<AnyElement> {
    if !page.compact {
        return None;
    }
    let p = c.p;
    let reduced = ui::Motion::system().is_reduced();
    let (shown, animating) = ui::fade_value(&ElementId::from("chat-drawer"), f32::from(page.drawer), DRAWER_MS);
    if animating {
        window.request_animation_frame();
    }
    if shown <= 0.001 && !page.drawer {
        return None;
    }
    let slide = if reduced { 0. } else { (1. - shown) * SIDEBAR_W };
    let scrim = div()
        .id("chat-scrim")
        .absolute()
        .inset_0()
        .bg(ui::hex_alpha(if p.dark { p.bg } else { crate::theme::hex("#444139") }, 0.18 * shown))
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| {
            page.drawer = false;
            cx.notify();
        })));
    let panel = panel(page, c, cx, true);
    Some(
        div()
            .absolute()
            .inset_0()
            .child(scrim)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(px(-slide))
                    .opacity(if reduced { shown } else { 1. })
                    .child(panel),
            )
            .into_any_element(),
    )
}

pub(super) fn panel(page: &ChatPage, c: &Ctx, cx: &mut Context<ChatPage>, in_drawer: bool) -> Stateful<Div> {
    let p = c.p;
    let drag =
        (!in_drawer).then(|| ui::drag_area("chat-sidebar-drag").absolute().top_0().left_0().right_0().h(px(HEADER_H)));
    let action = if in_drawer {
        ui::circle_control(
            "chat-history-close",
            Ic::X,
            CircleSize::Small,
            CircleAppearance { background: None, foreground: p.text, border: None, hover: Some(p.seg) },
        )
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.toggle_drawer(cx))))
    } else {
        ui::circle_control(
            "chat-new",
            Ic::Plus,
            CircleSize::Small,
            CircleAppearance { background: None, foreground: p.text, border: None, hover: Some(p.seg) },
        )
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.new_chat(cx))))
    }
    .cursor_pointer();
    let header = div()
        .h(px(HEADER_H))
        .flex_none()
        .px(px(24.))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(p.border)
        .child(ui::text(c.t(Tr::ChatHistory), 14., FontWeight::SEMIBOLD, p.text).line_height(px(20.)))
        .child(div().mr(px(-10.)).child(action));
    let mut list = div().w_full().flex().flex_col().gap(px(4.)).px(px(14.)).pt(px(12.)).pb(px(12.));
    if in_drawer {
        list = list.child(new_chat_row(c, cx));
    } else if page.current.is_none() {
        list = list.child(draft_row(c));
    }
    for session in &page.sessions {
        list = list.child(session_row(page, session.id, &session.title, c, cx));
    }
    div()
        .id(if in_drawer { "chat-drawer-panel" } else { "chat-sidebar" })
        .relative()
        .w(px(SIDEBAR_W))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .bg(p.side)
        .border_l_1()
        .border_color(p.border)
        .occlude()
        .children(drag)
        .child(header)
        .child(div().id("chat-sessions").flex_1().min_h(px(0.)).overflow_y_scroll().child(list))
}

fn row_shell(id: ElementId, selected: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id.clone())
        .relative()
        .h(px(ROW_H))
        .w_full()
        .flex_none()
        .px(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .rounded(px(10.))
        .cursor_pointer()
        .on_hover(ui::hover_tracker(id.clone()))
        .when(selected, |d| d.bg(p.nav_on))
        .child(ui::hover_fill_if(&id, Some(10.), p.nav_on, !selected))
}

fn label(text: String, selected: bool, p: &Palette) -> Div {
    ui::text(
        text,
        13.,
        if selected { FontWeight::MEDIUM } else { FontWeight::NORMAL },
        if selected { p.text } else { p.muted },
    )
    .line_height(px(19.))
    .flex_1()
    .min_w(px(0.))
    .truncate()
}

/// The unsaved new chat, shown selected while nothing has been sent in it.
fn draft_row(c: &Ctx) -> Stateful<Div> {
    let p = c.p;
    row_shell(ElementId::from("chat-draft"), true, &p).child(label(c.t(Tr::ChatEmptyTitle).to_string(), true, &p))
}

fn new_chat_row(c: &Ctx, cx: &mut Context<ChatPage>) -> Stateful<Div> {
    let p = c.p;
    row_shell(ElementId::from("chat-new-row"), false, &p)
        .justify_start()
        .gap(px(10.))
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.new_chat(cx))))
        .child(ui::icon(Ic::Plus, 16., p.text))
        .child(ui::text(c.t(Tr::ChatEmptyTitle), 13., FontWeight::MEDIUM, p.text).line_height(px(19.)))
}

fn session_row(page: &ChatPage, id: i64, title: &str, c: &Ctx, cx: &mut Context<ChatPage>) -> Stateful<Div> {
    let p = c.p;
    let selected = page.current == Some(id);
    let key = ElementId::from(("chat-session", id as usize));
    let renaming = page.rename.as_ref().filter(|r| r.id == id);
    let menu_open = page.menu.as_ref().is_some_and(|m| m.id == id);
    let hovered = ui::is_hovered(&key);
    let mut row = row_shell(key, selected, &p);
    if let Some(rename) = renaming {
        return row.child(div().flex_1().min_w(px(0.)).child(ui::text_input(&p, &rename.input, None)));
    }
    row = row
        .on_click(ui::click_cb(ui::cbk(cx, move |page, _, cx| {
            if page.rename.is_none() {
                page.open_chat(id, cx);
            }
        })))
        .child(label(title.to_owned(), selected, &p));
    if selected || hovered || menu_open {
        row = row.child(actions_button(id, menu_open, page, c, cx));
    }
    row
}

fn actions_button(id: i64, open: bool, page: &ChatPage, c: &Ctx, cx: &mut Context<ChatPage>) -> impl IntoElement {
    let p = c.p;
    let button_id = ElementId::from(("chat-session-actions", id as usize));
    let label = c.t(Tr::ChatActions);
    let button = ui::circle_control_with_glyph(
        button_id.clone(),
        Ic::MoreV,
        CircleSize::Compact,
        13.,
        CircleAppearance { background: None, foreground: p.text, border: None, hover: Some(p.border_strong) },
    )
    .cursor_pointer()
    .role(Role::Button)
    .aria_label(label)
    .on_click(ui::click_cb(ui::cbk(cx, move |page, _, cx| {
        let recent = page
            .menu_closed
            .take()
            .is_some_and(|(closed, at)| closed == id && at.elapsed() < Duration::from_millis(200));
        page.menu = match &page.menu {
            Some(m) if m.id == id => None,
            _ if recent => None,
            _ => Some(RowMenu { id, armed: false }),
        };
        cx.notify();
    })));
    let menu = (open && page.menu.as_ref().is_some_and(|m| m.id == id)).then(|| menu_panel(id, page, c, cx));
    button.children(menu)
}

fn menu_panel(id: i64, page: &ChatPage, c: &Ctx, cx: &mut Context<ChatPage>) -> Div {
    let p = c.p;
    let armed = page.menu.as_ref().is_some_and(|m| m.armed);
    let key = ElementId::from(("chat-session-menu", id as usize));
    let close = ui::cbk(cx, |page, _, cx| {
        page.menu_closed = page.menu.take().map(|m| (m.id, Instant::now()));
        cx.notify();
    });
    let rename = ui::menu_row(ElementId::from((key.clone(), "rename")), &p)
        .on_click(ui::click_cb(ui::cbk(cx, move |page, window, cx| page.start_rename(id, window, cx))))
        .child(ui::text(c.t(Tr::ChatRename), 13., FontWeight::NORMAL, p.text).line_height(relative(1.)));
    let (delete_text, tone) = if armed {
        (c.t(Tr::TranscriptionDeleteConfirm), if p.dark { p.danger } else { p.danger_text })
    } else {
        (c.t(Tr::TranscriptionDelete), if p.dark { p.danger } else { p.danger_text })
    };
    let delete = ui::menu_row_staying(ElementId::from((key.clone(), "delete")), &p)
        .on_click(ui::click_cb(ui::cbk(cx, move |page, _, cx| {
            if page.menu.as_ref().is_some_and(|m| m.armed) {
                page.delete_chat(id, cx);
            } else if let Some(menu) = &mut page.menu {
                menu.armed = true;
                cx.notify();
            }
        })))
        .child(ui::text(delete_text, 13., FontWeight::NORMAL, tone).line_height(relative(1.)));
    let panel = ui::menu_panel(key.clone(), &p, close).w(px(200.)).child(rename).child(delete);
    ui::float_menu(ElementId::from((key, "float")), 30., panel)
}
