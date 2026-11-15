//! The meeting list: every saved transcript as one row, newest first, with the running session on top.

use dito_core::i18n::Tr;
use dito_core::store::TranscriptSummary;
use dito_core::transcription::SessionState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TranscriptionPage, clock, date_time, open_row, row_fill};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, HOVER_MS, text};

/// `design:tools/transcripts` measurements.
const ROW_PAD_X: f32 = 22.;
const ROW_PAD_Y: f32 = 15.;
const ARROW_SHIFT: f32 = 2.;

pub(super) fn view(page: &mut TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    let live = live_id(page);
    let rows = order(&page.list, live);
    if rows.is_empty() {
        return ui::card(p)
            .p(px(24.))
            .child(text(c.t(Tr::TranscriptionEmpty), 13., FontWeight::NORMAL, p.muted).line_height(px(20.)));
    }
    let mut list =
        div().id("transcript-list").size_full().flex().flex_col().overflow_y_scroll().track_scroll(&page.list_scroll);
    for (i, row) in rows.iter().enumerate() {
        list = list.child(self_row(c, cx, row, live, &page.list_scroll));
        if i + 1 < rows.len() {
            list = list.child(div().w_full().h(px(1.)).flex_none().bg(p.border));
        }
    }
    ui::card(p).flex_1().min_h(px(0.)).overflow_hidden().child(list)
}

/// The transcript a running session records, while it records.
fn live_id(page: &TranscriptionPage) -> Option<i64> {
    match page.status.state {
        SessionState::Recording { id, .. } => Some(id),
        _ => None,
    }
}

/// The running session sits on top so the user finds it first, the rest newest first.
fn order(list: &[TranscriptSummary], live: Option<i64>) -> Vec<TranscriptSummary> {
    let mut rows: Vec<TranscriptSummary> = list.to_vec();
    if let Some(live) = live
        && let Some(at) = rows.iter().position(|row| row.id == live)
    {
        let row = rows.remove(at);
        rows.insert(0, row);
    }
    rows
}

fn self_row(
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    row: &TranscriptSummary,
    live: Option<i64>,
    scroll: &ScrollHandle,
) -> Stateful<Div> {
    let p = c.p;
    let id = row.id;
    let key = ElementId::from(("transcript-row", id as usize));
    let running = live == Some(id);
    let hovered = ui::is_hovered(&key);
    let title: SharedString =
        if row.title.trim().is_empty() { c.t(Tr::TranscriptionUntitled) } else { row.title.clone().into() };
    let mut head = div().flex().items_center().gap(px(10.)).child(text(title, 16., FontWeight::SEMIBOLD, p.text));
    if running {
        head = head.child(
            div().flex().items_center().gap(px(5.)).child(div().size(px(6.)).rounded_full().bg(p.danger)).child(text(
                c.t(Tr::TranscriptionRecording),
                12.,
                FontWeight::MEDIUM,
                p.danger_text,
            )),
        );
    } else if let Some(end) = row.ended_ts {
        head = head.child(badge(&p, clock((end - row.started_ts).max(0) as u64 * 1000)));
    }
    let mut column = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(4.)).child(head);
    if !row.preview.is_empty() {
        column = column.child(text(row.preview.clone(), 13., FontWeight::NORMAL, p.muted).truncate());
    }
    let arrow = ui::tweened(
        ElementId::from((key.clone(), "arrow")),
        if hovered { ARROW_SHIFT } else { 0. },
        HOVER_MS,
        move |shift| {
            div().ml(px(shift)).flex_none().child(ui::icon(
                Ic::ArrowRight,
                16.,
                ui::mix_rgb(p.muted, p.text, shift / ARROW_SHIFT),
            ))
        },
    );
    let tail = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(text(date_time(row.started_ts), 13., FontWeight::NORMAL, p.muted))
        .child(arrow);
    let fill = if running { p.accent_soft } else { p.sub };
    let scroll = scroll.clone();
    div()
        .id(key.clone())
        .w_full()
        .flex_none()
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .relative()
        .flex()
        .items_center()
        .gap(px(16.))
        .cursor_pointer()
        .on_hover(ui::hover_tracker(key.clone()))
        .on_click(open_row(cx, id))
        .child(ui::tweened(
            ElementId::from((key, "row-fill")),
            f32::from(running || hovered),
            HOVER_MS,
            move |opacity| row_fill(scroll.clone(), fill.opacity(opacity)),
        ))
        .child(column)
        .child(tail)
}

/// The duration badge beside a title (`design:tools/transcripts`).
pub(super) fn badge(p: &Palette, label: String) -> Div {
    div()
        .h(px(20.))
        .px(px(7.))
        .flex_none()
        .flex()
        .items_center()
        .rounded(px(6.))
        .bg(p.sub)
        .border_1()
        .border_color(p.border_strong)
        .child(text(label, 11., FontWeight::MEDIUM, p.model_badge_text).line_height(px(13.)))
}
