//! The conversation: the user's bubbles, the replies in Markdown, the generating status with its sweeping band, the
//! empty state and the pill that returns to the newest reply (`design:tools/chat`, 23.05.1 to 23.05.4, 23.05.6).

use std::time::Instant;

use dito_core::i18n::Tr;
use dito_core::store::{Attachment, Author, ChatMessageRow, MessageState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::switch::{Phase, Pose, Poses, turns};
use super::{ChatPage, markdown_view};
use crate::assets::Ic;
use crate::settings::{Ctx, Edge, edge_fade};
use crate::theme::Palette;
use crate::ui::{self, CircleAppearance, CircleSize};

const BUBBLE_MAX_W: f32 = 410.;
const TURN_GAP: f32 = 28.;
const EDGE_FADE: f32 = 28.;
const SWEEP_MS: f32 = 1600.;
const BAND_SHARE: f32 = 0.4;
const WAIT_FIRST_MS: f32 = 3000.;
const WAIT_SLOT_MS: f32 = 4000.;
const WAIT_FADE_MS: f32 = 240.;

pub(super) fn viewport(page: &ChatPage, c: &Ctx, cx: &mut Context<ChatPage>) -> AnyElement {
    let p = c.p;
    let now = Instant::now();
    let poses = page.switching.as_ref().and_then(|s| s.anim.poses(s.anim.elapsed_ms(now)));
    let (rows, docs) = match (&page.switching, poses.as_ref().map(|p| p.phase)) {
        (Some(sw), Some(Phase::Out)) => (&sw.old_rows, &sw.old_docs),
        _ => (&page.rows, &page.docs),
    };
    let shell = div().flex_1().min_h(px(0.)).relative();
    if rows.is_empty() {
        return shell.child(empty_state(c)).into_any_element();
    }
    let all = turns(rows);
    let animated = poses.as_ref().map_or(0, |p| p.turns.len());
    let first_animated = all.len().saturating_sub(animated);
    let mut column = div().w_full().flex().flex_col().gap(px(TURN_GAP)).pb(px(24.));
    for (t, range) in all.iter().enumerate() {
        let pose = poses.as_ref().and_then(|p| pose_of(p, t, first_animated));
        let mut turn = div().w_full().flex().flex_col().gap(px(24.));
        for row in &rows[range.clone()] {
            turn = match row.author {
                Author::User => turn.child(user_bubble(row, c, &ElementId::from(("chat-user", row.id as usize)))),
                Author::Assistant => turn.child(reply(page, row, docs.get(&row.id), c, cx)),
            };
        }
        column = column.child(match pose {
            Some(Pose { opacity, rise }) => turn.opacity(opacity).relative().top(px(rise)),
            None => turn,
        });
    }
    let scroll = div()
        .id("chat-scroll")
        .size_full()
        .overflow_y_scroll()
        .track_scroll(&page.scroll)
        .on_scroll(cx.listener(|_, _, window, cx| {
            window.refresh();
            cx.notify();
        }))
        .child(column);
    let hover_id = ElementId::from("chat-scroll-area");
    let extent = ui::ScrollExtent {
        viewport: f32::from(page.scroll.bounds().size.height),
        max: f32::from(page.scroll.max_offset().y),
        scrolled: -f32::from(page.scroll.offset().y),
    };
    let overflowing = extent.max > 0.;
    let top_fade = (extent.scrolled / EDGE_FADE).clamp(0., 1.);
    let bottom_fade = ((extent.max - extent.scrolled) / EDGE_FADE).clamp(0., 1.);
    let mut area = shell
        .id(hover_id.clone())
        .on_hover(ui::hover_tracker(hover_id.clone()))
        .child(scroll)
        .children((top_fade > 0.001).then(|| edge_fade(Edge::Top, p.bg, EDGE_FADE, 0.).opacity(top_fade)))
        .children((bottom_fade > 0.001).then(|| edge_fade(Edge::Bottom, p.bg, EDGE_FADE, 0.).opacity(bottom_fade)))
        .child(ui::overlay_thumb("chat-thumb", &p, ui::is_hovered(&hover_id), extent, 0.));
    if overflowing && !page.at_bottom() && poses.is_none() {
        area = area.child(jump_pill(c, cx));
    }
    area.into_any_element()
}

/// The pose of turn `t`: turns above the animated ones take the first pose, so the whole list fades as one.
fn pose_of(poses: &Poses, t: usize, first_animated: usize) -> Option<Pose> {
    let first = *poses.turns.first()?;
    Some(if t < first_animated { first } else { *poses.turns.get(t - first_animated).unwrap_or(&first) })
}

fn empty_state(c: &Ctx) -> Div {
    let p = c.p;
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .child(ui::text(c.t(Tr::ChatEmptyTitle), 22., FontWeight::BOLD, p.text).line_height(px(30.)))
        .child(ui::text(c.t(Tr::ChatEmptyHint), 13., FontWeight::NORMAL, p.muted).line_height(px(20.)))
}

fn jump_pill(c: &Ctx, cx: &mut Context<ChatPage>) -> Div {
    let p = c.p;
    let id = ElementId::from("chat-jump-latest");
    div().absolute().left_0().right_0().bottom(px(12.)).flex().justify_center().child(
        div()
            .id(id.clone())
            .relative()
            .h(px(32.))
            .px(px(16.))
            .flex()
            .items_center()
            .rounded_full()
            .bg(p.card)
            .border_1()
            .border_color(p.border_strong)
            .cursor_pointer()
            .on_hover(ui::hover_tracker(id.clone()))
            .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| {
                page.scroll.scroll_to_bottom();
                cx.notify();
            })))
            .child(ui::hover_fill(&id, None, p.seg))
            .child(ui::text(c.t(Tr::ChatJumpLatest), 13., FontWeight::NORMAL, p.text).line_height(px(18.))),
    )
}

fn user_bubble(row: &ChatMessageRow, c: &Ctx, id: &ElementId) -> Div {
    let p = c.p;
    let mut bubble =
        div().max_w(px(BUBBLE_MAX_W)).px(px(16.)).py(px(12.)).rounded(px(16.)).bg(p.sub).flex().flex_col().gap(px(8.));
    if !row.attachments.is_empty() {
        let mut chips = div().flex().flex_wrap().gap(px(6.));
        for attachment in &row.attachments {
            chips = chips.child(sent_chip(attachment, &p));
        }
        bubble = bubble.child(chips);
    }
    if !row.content.is_empty() {
        bubble = bubble.child(div().text_size(px(14.)).line_height(px(22.)).child(ui::selectable_text(
            &p,
            id.clone(),
            row.content.clone(),
            14.,
            FontWeight::NORMAL,
            p.text,
        )));
    }
    div().w_full().flex().justify_end().child(bubble)
}

fn sent_chip(attachment: &Attachment, p: &Palette) -> Div {
    let glyph = match attachment {
        Attachment::File { .. } => Ic::FileText,
        Attachment::Meeting { .. } => Ic::AudioLines,
    };
    div()
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(8.))
        .bg(p.card)
        .child(ui::icon(glyph, 12., p.muted))
        .child(ui::text(attachment.name().to_owned(), 12., FontWeight::MEDIUM, p.text).max_w(px(180.)).truncate())
}

fn reply(
    page: &ChatPage,
    row: &ChatMessageRow,
    doc: Option<&std::rc::Rc<super::markdown::Document>>,
    c: &Ctx,
    cx: &mut Context<ChatPage>,
) -> Div {
    let p = c.p;
    let live = page.generation.as_ref().is_some_and(|g| g.message_id == row.id);
    let last = page.rows.last().is_some_and(|r| r.id == row.id);
    let mut head =
        div().h(px(18.)).flex().items_center().gap(px(8.)).child(
            ui::text(row.model.clone().unwrap_or_default(), 13., FontWeight::MEDIUM, p.text).line_height(px(18.)),
        );
    if live {
        let generation = page.generation.as_ref();
        let elapsed = generation.map_or(0., |g| g.started.elapsed().as_secs_f32() * 1000.);
        let waiting = doc.is_none_or(|d| d.blocks().next().is_none());
        let (label, fade) = match generation.map(|g| g.ready()) {
            Some(None) => (Tr::ChatLoadingModel, 1.),
            ready => {
                let since_ready = ready.flatten().map_or(elapsed, |at| at.elapsed().as_secs_f32() * 1000.);
                status_phrase(waiting, since_ready)
            }
        };
        head = head.child(generating_label(&c.t(label), &p, elapsed).opacity(fade));
    } else if matches!(row.state, MessageState::Stopped | MessageState::Failed) {
        let (label, dot) = if row.state == MessageState::Stopped {
            (c.t(Tr::ChatStopped), p.muted)
        } else {
            (c.t(Tr::ChatFailed), p.danger)
        };
        head = head
            .child(div().size(px(5.)).rounded_full().bg(dot))
            .child(ui::text(label, 12., FontWeight::NORMAL, p.muted).line_height(px(18.)));
        if last && !page.generating() {
            head = head.child(retry_button(c, cx));
        }
    }
    let mut body = div().w_full().flex().flex_col().gap(px(12.));
    if let Some(doc) = doc {
        let id = ElementId::from(("chat-reply", row.id as usize));
        body = body.child(markdown_view::blocks(doc.blocks(), &p, &id, cx));
    }
    if live {
        body = body.child(div().w(px(2.)).h(px(16.)).rounded(px(1.)).bg(p.accent));
    }
    let mut column = div().w_full().flex().flex_col().gap(px(12.)).child(head);
    if row.state == MessageState::Failed
        && let Some(error) = &row.error
    {
        column = column
            .child(ui::text(ui::error_line(c.lang, error), 12., FontWeight::NORMAL, p.muted).line_height(px(18.)));
    }
    column.child(body)
}

fn retry_button(c: &Ctx, cx: &mut Context<ChatPage>) -> impl IntoElement {
    let p = c.p;
    let id = ElementId::from("chat-retry");
    let label = c.t(Tr::ChatRetry);
    ui::tip_label(id.clone(), label.clone());
    ui::circle_control(
        id.clone(),
        Ic::RotateCcw,
        CircleSize::Compact,
        CircleAppearance { background: None, foreground: p.muted, border: None, hover: Some(p.seg) },
    )
    .cursor_pointer()
    .role(Role::Button)
    .aria_label(label)
    .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.retry(cx))))
    .child(ui::tip_above(&p, &id))
}

/// Before the first words arrive, a long read of the conversation looks like a hang, so the status changes its words
/// every few seconds and fades each new line in. Once the reply is being written it says so and stays.
fn status_phrase(waiting: bool, elapsed_ms: f32) -> (Tr, f32) {
    const PHRASES: [Tr; 4] =
        [Tr::ChatWaitingReading, Tr::ChatWaitingThinking, Tr::ChatWaitingPatience, Tr::ChatWaitingWorking];
    if !waiting || elapsed_ms < WAIT_FIRST_MS {
        return (Tr::ChatGenerating, 1.);
    }
    let since = elapsed_ms - WAIT_FIRST_MS;
    let slot = (since / WAIT_SLOT_MS) as usize % PHRASES.len();
    let fade = if ui::Motion::system().is_reduced() { 1. } else { (since % WAIT_SLOT_MS / WAIT_FADE_MS).min(1.) };
    (PHRASES[slot], fade)
}

/// The label is muted; an accent band 40 percent of its width sweeps left to right, linear, on a loop. Reduce Motion
/// keeps the muted label alone.
fn generating_label(label: &str, p: &Palette, elapsed_ms: f32) -> Div {
    let size = 12.;
    let line = px(18.);
    let weight = FontWeight::NORMAL;
    let row = |color: Hsla| div().h(line).flex().items_center().child(ui::text(label.to_owned(), size, weight, color));
    let width = ui::text_width(label, size, weight);
    let mut el = div().relative().h(line).flex_none().child(row(p.muted));
    if !ui::Motion::system().is_reduced() {
        let band = width * BAND_SHARE;
        let phase = (elapsed_ms % SWEEP_MS) / SWEEP_MS;
        let start = -band + (width + band) * phase;
        let (from, to) = (start.max(0.).round(), (start + band).min(width).round());
        if to > from {
            el = el.child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(from))
                    .w(px(to - from))
                    .h(line)
                    .overflow_hidden()
                    .child(div().ml(px(-from)).child(row(p.accent_text))),
            );
        }
    }
    el
}
