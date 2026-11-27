//! The panel of the live meeting notes: the title, the notes or the conversation, and the question field
//! (`design:tools/recording/live-notes`, 23.02.8).

use dito_core::i18n::{Tr, fmt};
use dito_core::meeting_notes::{Item, Notes, NotesPhase, NotesView, Unavailable};
use dito_core::store::{Author, ChatMessageRow, MessageState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::timefmt::hhmm;
use crate::tools::chat::markdown::Document;
use crate::tools::chat::markdown_view;
use crate::tools::transcription::TranscriptionPage;
use crate::ui::{self, BtnKind, TextAreaStyle, text};

pub(in crate::tools::transcription) const PANEL_W: f32 = 340.;
const PAD: f32 = 16.;
const COMPOSER_MIN_H: f32 = 39.;
const ROUND: f32 = 28.;
const NOTES_SCROLL: &str = "meeting-notes-scroll";

/// The panel, `PANEL_W` wide beside the transcript or as wide as the page when notes and transcript take turns.
pub(in crate::tools::transcription) fn view(
    page: &TranscriptionPage,
    c: &Ctx,
    full: bool,
    cx: &mut Context<TranscriptionPage>,
) -> Div {
    let p = c.p;
    let ask = &page.notes.ask;
    let conversation = ask.conversation && !ask.rows.is_empty();
    let body = if conversation { conversation_body(page, c) } else { notes_body(page, c, cx) };
    let mut shell = div()
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .px(px(PAD))
        .pt(px(12.))
        .pb(px(14.))
        .rounded(px(16.))
        .bg(p.sub)
        .border_1()
        .border_color(p.border);
    shell = if full { shell.w_full() } else { shell.w(px(PANEL_W)) };
    shell.child(header(page, c, conversation, cx)).child(body).child(composer(page, c, cx))
}

fn header(page: &TranscriptionPage, c: &Ctx, conversation: bool, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = c.p;
    let title = c.t(if conversation { Tr::TranscriptionNotesAskTitle } else { Tr::TranscriptionNotesTitle });
    let mut row = div()
        .h(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .child(text(title, 15., FontWeight::BOLD, p.text).line_height(px(22.)).min_w(px(0.)).truncate());
    if conversation {
        row = row.child(
            div()
                .flex_none()
                .flex()
                .items_center()
                // The way back and the way on stand 8 pt apart, and neither repeats itself in words.
                .gap(px(8.))
                .child(back_to_notes(c, cx))
                .child(continue_in_chat(page, c, cx)),
        );
    }
    row
}

fn back_to_notes(c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Stateful<Div> {
    let p = c.p;
    let id = ElementId::from("meeting-notes-back");
    let label = c.t(Tr::TranscriptionNotesBack);
    div()
        .id(id.clone())
        .relative()
        .h(px(ROUND))
        .px(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .rounded_full()
        .bg(p.input_bg)
        .border_1()
        .border_color(p.border_strong)
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label.clone())
        .on_hover(ui::hover_tracker(id.clone()))
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.ask_back(cx))))
        .child(ui::hover_fill(&id, None, p.seg))
        .child(text(label, 11., FontWeight::MEDIUM, p.text).line_height(px(18.)))
}

fn round(id: ElementId, glyph: Ic, p: &Palette) -> Stateful<Div> {
    div()
        .id(id.clone())
        .relative()
        .size(px(ROUND))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(p.input_bg)
        .border_1()
        .border_color(p.border_strong)
        .on_hover(ui::hover_tracker(id.clone()))
        .child(ui::hover_fill(&id, None, p.seg))
        .child(ui::icon(glyph, 14., p.text))
}

/// One round button that goes on in Chat. It leaves no words next to it.
fn continue_in_chat(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> impl IntoElement {
    let p = c.p;
    let id = ElementId::from("meeting-notes-continue");
    let label = c.t(Tr::TranscriptionNotesContinueChat);
    ui::tip_label(id.clone(), label.clone());
    let _ = page;
    round(id.clone(), Ic::External, &p)
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.ask_continue(cx))))
        .child(ui::tip_above(&p, &id))
}

fn notes_body(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Stateful<Div> {
    let p = c.p;
    let view = &page.notes.view;
    let mut column = div().w_full().flex().flex_col().pt(px(12.)).pb(px(8.));
    let message = |tr: Tr| text(c.t(tr), 12., FontWeight::NORMAL, p.muted).line_height(px(20.));
    match (&view.phase, &view.notes) {
        (NotesPhase::Unavailable(Unavailable::ModelFailed(_)), _) => {
            column = column.gap(px(12.)).child(message(Tr::TranscriptionNotesModelFailed)).child(div().flex().child(
                ui::button(
                    "meeting-notes-model-settings",
                    &p,
                    &c.t(Tr::TranscriptionNotesOpenModel),
                    BtnKind::Secondary,
                    None,
                    ui::cbk(cx, |page, _, cx| page.open_chat_model_settings(cx)),
                ),
            ));
        }
        (_, Some(notes)) if !notes.is_empty() => {
            column = notes_column(column, notes, view, c);
        }
        (NotesPhase::Working { loading_model: true }, _) => column = column.child(message(Tr::ChatLoadingModel)),
        (NotesPhase::Working { .. }, _) => column = column.child(message(Tr::TranscriptionNotesWorking)),
        (_, _) => column = column.child(message(Tr::TranscriptionNotesWaiting)),
    }
    scroller(NOTES_SCROLL, &page.notes.scroll, column, &p)
}

fn notes_are_stale(view: &NotesView) -> bool {
    view.stale.memo || !view.ended && view.stale.hint
}

/// Draws the hint and the memo in the layout the panel had before the memo grew groups; `design:tools/recording/live-notes`
/// gives them their own blocks next.
fn notes_column(mut column: Div, notes: &Notes, view: &NotesView, c: &Ctx) -> Div {
    let p = c.p;
    let off = dito_platform::local_utc_offset_secs();
    let clock = |ms: u64| hhmm(notes.started_ts + (ms / 1000) as i64, off);
    let label = |s: SharedString| text(s, 11., FontWeight::BOLD, p.muted).line_height(px(18.));
    let aside = |s: String| text(s, 11., FontWeight::NORMAL, p.muted).line_height(px(18.));
    let dated = |item: &Item| match &item.at {
        Some(at) => format!("{at}\u{3000}{}", item.text),
        None => item.text.clone(),
    };
    if let Some(now) = &notes.now
        && !view.ended
    {
        column = column
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label(c.t(Tr::TranscriptionNotesRecent)))
                    .child(aside(fmt::transcription_notes_until(c.lang, &clock(now.covered_ms)))),
            )
            .child(div().pt(px(3.)).child(text(now.topic.clone(), 16., FontWeight::BOLD, p.text).line_height(px(23.))));
    }
    let memo = &notes.memo;
    if !memo.is_empty() {
        column = column.child(div().mt(px(12.)).h(px(1.)).w_full().bg(p.border)).child(
            div()
                .pt(px(11.))
                .flex()
                .items_center()
                .justify_between()
                .child(label(c.t(Tr::TranscriptionNotesEarlier)))
                .children(
                    memo.span_ms
                        .map(|(from, to)| aside(fmt::transcription_notes_range(c.lang, &clock(from), &clock(to)))),
                ),
        );
        let mut points = div().pt(px(4.)).flex().flex_col();
        for item in memo.decided.iter().chain(&memo.todo).chain(&memo.open).chain(&memo.flow) {
            points = points.child(text(dated(item), 12., FontWeight::NORMAL, p.text).line_height(px(17.)));
        }
        column = column.child(points);
    }
    if notes_are_stale(view) {
        column = column.child(div().pt(px(10.)).child(aside(c.t(Tr::TranscriptionNotesStale).to_string())));
    }
    column
}

fn conversation_body(page: &TranscriptionPage, c: &Ctx) -> Stateful<Div> {
    let p = c.p;
    let ask = &page.notes.ask;
    let mut column = div().w_full().flex().flex_col().gap(px(14.)).pt(px(12.)).pb(px(8.));
    for row in &ask.rows {
        column = column.child(match row.author {
            Author::User => question(row, &p),
            Author::Assistant => answer(row, ask.answers.get(&row.id), ask.generating(), c),
        });
    }
    scroller("meeting-notes-ask-scroll", &ask.scroll, column, &p)
}

fn question(row: &ChatMessageRow, p: &Palette) -> Div {
    div().w_full().flex().justify_end().child(
        div()
            .max_w(px(240.))
            .px(px(11.))
            .py(px(7.))
            .rounded(px(10.))
            .bg(p.seg)
            .child(text(row.content.clone(), 12., FontWeight::NORMAL, p.text).line_height(px(20.))),
    )
}

fn answer(row: &ChatMessageRow, doc: Option<&Document>, generating: bool, c: &Ctx) -> Div {
    let p = c.p;
    if row.content.is_empty() {
        let (label, color) = match row.state {
            MessageState::Failed => (c.t(Tr::ChatFailed), p.danger_text),
            MessageState::Stopped => (c.t(Tr::ChatStopped), p.muted),
            _ if generating => (c.t(Tr::ChatGenerating), p.muted),
            _ => (c.t(Tr::ChatFailed), p.muted),
        };
        return div().child(text(label, 12., FontWeight::NORMAL, color).line_height(px(20.)));
    }
    match doc {
        Some(doc) => markdown_view::document_on_sub(doc, &p),
        None => div(),
    }
}

fn scroller(id: &'static str, handle: &ScrollHandle, content: Div, p: &Palette) -> Stateful<Div> {
    let hover = ElementId::from((id, 1usize));
    let extent = ui::ScrollExtent {
        viewport: f32::from(handle.bounds().size.height),
        max: f32::from(handle.max_offset().y),
        scrolled: -f32::from(handle.offset().y),
    };
    div()
        .id(hover.clone())
        .relative()
        .flex_1()
        .min_h(px(0.))
        .on_hover(ui::hover_tracker(hover.clone()))
        .child(div().id(id).size_full().overflow_y_scroll().track_scroll(handle).child(content))
        .child(ui::overlay_thumb((id, 2usize), p, ui::is_hovered(&hover), extent, 0.))
}

fn composer(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = c.p;
    let ask = &page.notes.ask;
    let empty = ask.composer.read(cx).value().trim().is_empty();
    let blocked = empty || ask.generating();
    let label = c.t(Tr::ChatSend);
    let id = ElementId::from("meeting-notes-send");
    let send = round(id, Ic::Send, &p).role(Role::Button).aria_label(label);
    let send = if blocked {
        send.opacity(0.45)
    } else {
        send.cursor_pointer().on_click(ui::click_cb(ui::cbk(cx, |page, window, cx| page.ask_send(window, cx))))
    };
    let notice = ask.notice.map(|tr| {
        div().w_full().pb(px(8.)).child(text(c.t(tr), 11., FontWeight::NORMAL, p.muted).line_height(px(16.)))
    });
    div().w_full().flex_none().flex().flex_col().children(notice).child(
        div()
            .w_full()
            .min_h(px(COMPOSER_MIN_H))
            .pl(px(11.))
            .pr(px(5.))
            .py(px(5.))
            .flex()
            .items_end()
            .gap(px(8.))
            .rounded(px(10.))
            .bg(p.input_bg)
            .border_1()
            .border_color(p.border_strong)
            .child(div().flex_1().min_w(px(0.)).min_h(px(ROUND)).flex().items_center().child(ui::text_area(
                &ask.composer,
                TextAreaStyle {
                    bg: ui::transparent(),
                    edge: None,
                    radius: 0.,
                    text_size: 12.,
                    line_height: 20.,
                    pad: [0., 0., 0., 0.],
                    fill: false,
                    reserve_bottom: 0.,
                },
            )))
            .child(send),
    )
}
