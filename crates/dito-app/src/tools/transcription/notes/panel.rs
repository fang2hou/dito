//! The panel of the live meeting notes: the title, the notes or the conversation, and the question field
//! (`design:tools/recording/live-notes`, 23.02.8).

use std::fmt::Write as _;

use dito_core::i18n::{Lang, Tr, fmt};
use dito_core::meeting_notes::{Hint, Item, MAX_TRAIL, Memo, Notes, NotesPhase, NotesView, Unavailable};
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
use crate::ui::{self, BtnKind, CircleSize, TextAreaStyle, text};

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
    let body = if conversation {
        conversation_body(page, c).into_any_element()
    } else {
        notes_body(page, c, cx).into_any_element()
    };
    let mut shell = div()
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .px(px(PAD))
        .pt(px(if conversation { 12. } else { 11. }))
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
    let view = page.notes.panel_view();
    let title = c.t(if conversation { Tr::TranscriptionNotesAskTitle } else { Tr::TranscriptionNotesTitle });
    let mut row = div()
        .h(px(if conversation { 28. } else { 24. }))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(if conversation { 12. } else { 8. }))
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
                .child(continue_in_chat(c, cx)),
        );
    } else if view
        .notes
        .as_ref()
        .is_some_and(|notes| current_topic(notes, view.ended).is_some() || !notes.memo.is_empty())
    {
        row = row.child(copy_notes(c, cx));
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
fn continue_in_chat(c: &Ctx, cx: &mut Context<TranscriptionPage>) -> impl IntoElement {
    let p = c.p;
    let id = ElementId::from("meeting-notes-continue");
    let label = c.t(Tr::TranscriptionNotesContinueChat);
    ui::tip_label(id.clone(), label.clone());
    round(id.clone(), Ic::External, &p)
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.ask_continue(cx))))
        .child(ui::tip_above(&p, &id))
}

fn copy_notes(c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Stateful<Div> {
    let id = ElementId::from("meeting-notes-copy");
    let label = c.t(Tr::TranscriptionNotesCopy);
    let lang = c.lang;
    ui::tip_label(id.clone(), label.clone());
    ui::copy_button_with(
        id.clone(),
        &c.p,
        CircleSize::Compact,
        ui::cbk_copy(cx, move |page, _, cx| {
            let Some(contents) = copied_notes(page.notes.panel_view(), lang, dito_platform::local_utc_offset_secs())
            else {
                return false;
            };
            cx.write_to_clipboard(ClipboardItem::new_string(contents));
            true
        }),
    )
    .role(Role::Button)
    .aria_label(label)
    .child(ui::tip_above(&c.p, &id))
}

fn notes_body(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = c.p;
    let view = page.notes.panel_view();
    let offset = dito_platform::local_utc_offset_secs();
    let topics = view.notes.as_ref().filter(|notes| !view.ended && (notes.now.is_some() || !notes.trail.is_empty()));
    let memo = view.notes.as_ref().filter(|notes| !notes.memo.is_empty());
    let topic_stale = view.stale.hint && !view.ended;
    let mut panel = div().flex_1().min_h(px(0.)).flex().flex_col().pt(px(12.)).pb(px(12.));
    if topics.is_some() || topic_stale {
        let mut pinned = div().flex_none().w_full().flex().flex_col().gap(px(10.));
        if let Some(notes) = topics {
            pinned = pinned.child(topic_block(notes, c, offset));
        }
        if topic_stale {
            pinned = pinned.child(status_line(Tr::TranscriptionNotesTopicStale, c));
        }
        panel = panel.child(pinned);
        if memo.is_some() {
            panel = panel.child(div().mt(px(12.)).mb(px(12.)).flex_none().h(px(1.)).w_full().bg(p.border));
        }
    }
    let mut column = div().w_full().flex().flex_col().gap(px(10.));
    let message = |tr: Tr| text(c.t(tr), 12., FontWeight::NORMAL, p.muted).line_height(px(20.));
    if let NotesPhase::Unavailable(Unavailable::ModelFailed(_)) = view.phase {
        column =
            column.child(div().flex().flex_col().gap(px(12.)).child(message(Tr::TranscriptionNotesModelFailed)).child(
                div().flex().child(ui::button(
                    "meeting-notes-model-settings",
                    &p,
                    &c.t(Tr::TranscriptionNotesOpenModel),
                    BtnKind::Secondary,
                    None,
                    ui::cbk(cx, |page, _, cx| page.open_chat_model_settings(cx)),
                )),
            ));
    } else if topics.is_none() && memo.is_none() && !topic_stale && !view.stale.memo {
        column = column.child(message(match view.phase {
            NotesPhase::Working { loading_model: true } => Tr::ChatLoadingModel,
            NotesPhase::Working { .. } => Tr::TranscriptionNotesWorking,
            _ => Tr::TranscriptionNotesWaiting,
        }));
    }
    if let Some(notes) = memo {
        column = column.child(memo_block(notes, c, offset));
    }
    if view.stale.memo {
        let label = if memo.is_some() { Tr::TranscriptionNotesStale } else { Tr::TranscriptionNotesMemoFailed };
        let gap = if memo.is_none() && topics.is_some() { 10. } else { 0. };
        column = column.child(status_line(label, c).pt(px(gap)));
    }
    panel.child(scroller(NOTES_SCROLL, &page.notes.scroll, column, &p))
}

fn status_line(label: Tr, c: &Ctx) -> Div {
    text(c.t(label), 11., FontWeight::NORMAL, c.p.muted).line_height(px(18.))
}

fn current_topic(notes: &Notes, ended: bool) -> Option<&Hint> {
    notes.now.as_ref().filter(|_| !ended)
}

fn clock(notes: &Notes, ms: u64, offset: i32) -> String {
    hhmm(notes.started_ts + (ms / 1000) as i64, offset)
}

fn memo_range(notes: &Notes, lang: Lang, offset: i32) -> Option<String> {
    notes
        .memo
        .span_ms
        .map(|(from, to)| fmt::transcription_notes_range(lang, &clock(notes, from, offset), &clock(notes, to, offset)))
}

fn block_header(label: Tr, time: Option<String>, c: &Ctx) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(text(c.t(label), 11., FontWeight::BOLD, c.p.muted).line_height(px(18.)).min_w(px(0.)).truncate())
        .children(time.map(|time| text(time, 11., FontWeight::NORMAL, c.p.muted).line_height(px(18.)).flex_none()))
}

fn topic_block(notes: &Notes, c: &Ctx, offset: i32) -> Div {
    let mut block = div().flex().flex_col();
    if let Some(topic) = &notes.now {
        let until = fmt::transcription_notes_until(c.lang, &clock(notes, topic.covered_ms, offset));
        block = block.child(block_header(Tr::TranscriptionNotesCurrentTopic, Some(until), c)).child(
            div().pt(px(4.)).child(
                text(topic.topic.clone(), 16., FontWeight::BOLD, c.p.text)
                    .w_full()
                    .line_height(px(23.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .line_clamp(2),
            ),
        );
    }
    if !notes.trail.is_empty() {
        let trail = notes.trail.iter().rev().take(MAX_TRAIL).map(|hint| {
            let time = clock(notes, hint.covered_ms, offset);
            text(format!("{time}\u{3000}{}", hint.topic), 12., FontWeight::NORMAL, c.p.muted)
                .font_family(ui::family_for(&hint.topic))
                .line_height(px(18.))
                .truncate()
        });
        block = block.child(
            div().pt(px(if notes.now.is_some() { 6. } else { 0. })).flex().flex_col().gap(px(2.)).children(trail),
        );
    }
    block
}

fn memo_groups(memo: &Memo) -> impl Iterator<Item = (Tr, &[Item])> {
    [
        (Tr::TranscriptionNotesDecided, memo.decided.as_slice()),
        (Tr::TranscriptionNotesTodo, memo.todo.as_slice()),
        (Tr::TranscriptionNotesOpenQuestions, memo.open.as_slice()),
        (Tr::TranscriptionNotesFlow, memo.flow.as_slice()),
    ]
    .into_iter()
    .filter(|(_, items)| !items.is_empty())
}

fn memo_block(notes: &Notes, c: &Ctx, offset: i32) -> Div {
    div()
        .flex()
        .flex_col()
        .child(block_header(Tr::TranscriptionNotesEarlier, memo_range(notes, c.lang, offset), c))
        .children(memo_groups(&notes.memo).map(|(label, items)| {
            div()
                .pt(px(10.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(text(c.t(label), 11., FontWeight::MEDIUM, c.p.muted).line_height(px(18.)))
                .children(items.iter().map(|item| memo_item(item, c)))
        }))
}

fn memo_item(item: &Item, c: &Ctx) -> Div {
    let p = c.p;
    let body = text(item.text.clone(), 12., FontWeight::NORMAL, p.text).line_height(px(17.));
    let Some(at) = &item.at else {
        return body;
    };
    div()
        .flex()
        .gap(px(12.))
        .child(
            text(at.clone(), 12., FontWeight::NORMAL, p.muted)
                .font_family(ui::family_for(&item.text))
                .line_height(px(17.))
                .flex_none(),
        )
        .child(body.flex_1().min_w(px(0.)))
}

fn copied_notes(view: &NotesView, lang: Lang, offset: i32) -> Option<String> {
    let notes = view.notes.as_ref()?;
    let mut contents = String::new();
    if let Some(topic) = current_topic(notes, view.ended) {
        let title = fmt::transcription_notes_topic_until(lang, &clock(notes, topic.covered_ms, offset));
        let _ = writeln!(contents, "{title}\n- {}", topic.topic);
    }
    if !notes.memo.is_empty() {
        contents.push_str(lang.tr(Tr::TranscriptionNotesTitle));
        if let Some(range) = memo_range(notes, lang, offset) {
            let _ = write!(contents, " {range}");
        }
        contents.push('\n');
        for (label, items) in memo_groups(&notes.memo) {
            let _ = writeln!(contents, "{}", lang.tr(label));
            for item in items {
                contents.push_str("- ");
                if let Some(at) = &item.at {
                    let _ = write!(contents, "{at} ");
                }
                let _ = writeln!(contents, "{}", item.text);
            }
        }
    }
    (!contents.is_empty()).then(|| {
        contents.pop();
        contents
    })
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
    let send = round(id, Ic::Send, &p).bg(p.card).role(Role::Button).aria_label(label);
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
            .bg(p.card)
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

#[cfg(test)]
mod tests {
    use dito_core::i18n::Lang;
    use dito_core::meeting_notes::{Hint, Item, Memo, Notes, NotesPhase, NotesView, Stale};

    use super::copied_notes;

    #[test]
    fn copies_the_current_topic_and_nonempty_memo_groups_in_order() {
        let mut view = NotesView {
            phase: NotesPhase::Ready,
            notes: Some(Notes {
                now: Some(Hint { topic: "Retest".into(), covered_ms: 480_000 }),
                trail: vec![Hint { topic: "Past topic".into(), covered_ms: 420_000 }],
                memo: Memo {
                    decided: vec![Item { at: None, text: "Retest on Friday.".into() }],
                    flow: vec![Item { at: Some("14:02".into()), text: "Reviewed the scope.".into() }],
                    span_ms: Some((0, 300_000)),
                    ..Memo::default()
                },
                started_ts: 14 * 3600 + 2 * 60,
                ..Notes::default()
            }),
            stale: Stale::default(),
            ended: false,
            seq: 0,
        };
        let memo =
            "Meeting notes 14:02 to 14:07\nDecided\n- Retest on Friday.\nHow it went\n- 14:02 Reviewed the scope.";
        assert_eq!(copied_notes(&view, Lang::En, 0), Some(format!("Current topic (until 14:10)\n- Retest\n{memo}")));

        view.ended = true;
        assert_eq!(copied_notes(&view, Lang::En, 0).as_deref(), Some(memo));
        view.notes.as_mut().unwrap().memo = Memo::default();
        assert_eq!(copied_notes(&view, Lang::En, 0), None);
    }
}
