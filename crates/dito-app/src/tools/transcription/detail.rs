//! One transcript as an editable document: a fixed speaker column, paragraphs edited in place, and the whole
//! meeting's copy and delete in the header.

use dito_core::i18n::{Tr, fmt};
use dito_core::store::{ParagraphState, Speaker};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{OpenTranscript, ParagraphView, TranscriptionPage, clock, date_time, speaker_label};
use crate::assets::Ic;
use crate::settings::{Ctx, Edge, edge_fade};
use crate::theme::Palette;
use crate::timefmt::hhmm;
use crate::ui::{self, BtnKind, CircleSize, Textarea, TextareaState, cbk, cbk_copy, text};

/// `design:tools/detail` measurements.
const META_COL: f32 = 98.;
const TIME_COL: f32 = 36.;
const ROW_PAD_X: f32 = 20.;
const ROW_PAD_Y: f32 = 10.;
const BODY_SIZE: f32 = 14.;
const BODY_LINE: f32 = 22.;
const ACTION_COL: f32 = 32.;
const FADE: f32 = 24.;
/// The textarea's own inset around its text (its medium input padding plus the 1 pt border it keeps); plain text
/// paragraphs use the same inset so editing never moves the words.
const EDITOR_PAD_X: f32 = 12.;
const EDITOR_PAD_Y: f32 = 9.;
/// The textarea wraps this far short of its text area (gpui-base's `RIGHT_MARGIN`).
const EDITOR_WRAP_MARGIN: f32 = 10.;
/// The meta column and the hover copy button hang from the first text line, which starts `EDITOR_PAD_Y` below the row.
const META_TOP: f32 = EDITOR_PAD_Y;
const META_H: f32 = BODY_LINE + EDITOR_PAD_Y;
const ACTION_TOP: f32 = (2. * EDITOR_PAD_Y + BODY_LINE - ACTION_COL) / 2.;
const ACTION_H: f32 = ACTION_COL + ACTION_TOP;

pub(super) fn view(page: &TranscriptionPage, c: &Ctx, window: &mut Window, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    let Some(open) = &page.open else { return div() };
    let id = open.summary.id;
    let live = page.status.state.transcript_id() == Some(id);
    let mut column = div().size_full().flex().flex_col().gap(px(18.));
    // A live transcript has no title field and no delete: the dock holds its title while it records, and the
    // header only says what is being written and offers the way back to the list.
    column = if live {
        column.child(live_head(open, page, c, cx))
    } else {
        column
            .child(finished_head(page, c, cx))
            .child(div().w_full().child(ui::text_input(p, &open.title, None)))
            .child(meta_strip(open, c))
    };
    column.child(paragraph_card(open, page, c, window, cx))
}

/// The header of the transcript the session is writing: when it started, how much it has, and the way back.
fn live_head(open: &OpenTranscript, page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    let off = dito_platform::local_utc_offset_secs();
    let count =
        open.paragraphs.iter().filter(|v| v.data.state != ParagraphState::Listening).take(page.status.closed).count();
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.))
        .child(ui::page_title(p, c.t(Tr::NavTranscription)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(text(
                    fmt::transcription_started_at(c.lang, &hhmm(open.summary.started_ts, off)),
                    12.,
                    FontWeight::NORMAL,
                    p.muted,
                ))
                .child(super::list::badge(p, fmt::transcription_count(c.lang, &count.to_string())))
                .child(ui::button(
                    "transcript-list",
                    p,
                    &c.t(Tr::TranscriptionOverview),
                    BtnKind::Secondary,
                    Some(Ic::Clock),
                    cbk(cx, |page, _, cx| page.close_detail(cx)),
                )),
        )
}

/// The header of a finished transcript: the way back, copy, and delete.
fn finished_head(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.))
        .child(ui::button(
            "transcript-back",
            p,
            &c.t(Tr::TranscriptionBack),
            BtnKind::Secondary,
            Some(Ic::ArrowLeft),
            cbk(cx, |page, _, cx| page.close_detail(cx)),
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(ui::copy_button("transcript-copy-all", p, cbk_copy(cx, |page, _, cx| page.copy_open(cx))))
                .child(delete_button(page, c, cx)),
        )
}

/// The finished transcript's one strip of facts: when it was, how long it ran, how much it holds.
fn meta_strip(open: &OpenTranscript, c: &Ctx) -> Div {
    let p = &c.p;
    let count = fmt::transcription_count(c.lang, &open.paragraphs.len().to_string());
    let length = match open.summary.ended_ts {
        Some(end) => clock((end - open.summary.started_ts).max(0) as u64 * 1000),
        None => clock((crate::timefmt::now() - open.summary.started_ts).max(0) as u64 * 1000),
    };
    div()
        .w_full()
        .h(px(36.))
        .px(px(14.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.))
        .rounded(px(10.))
        .bg(p.sub)
        .child(text(
            fmt::transcription_started_at(c.lang, &date_time(open.summary.started_ts)),
            12.,
            FontWeight::NORMAL,
            p.muted,
        ))
        .child(div().flex().items_center().gap(px(10.)).child(super::list::badge(p, length)).child(text(
            count,
            12.,
            FontWeight::NORMAL,
            p.muted,
        )))
}

fn delete_button(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> AnyElement {
    let p = &c.p;
    let label = c.t(if page.delete_armed { Tr::TranscriptionDeleteConfirm } else { Tr::TranscriptionDelete });
    ui::button(
        "transcript-delete",
        p,
        &label,
        BtnKind::Danger,
        Some(Ic::Trash),
        cbk(cx, |page, _, cx| page.delete_open(cx)),
    )
    .into_any_element()
}

fn paragraph_card(
    open: &OpenTranscript,
    page: &TranscriptionPage,
    c: &Ctx,
    window: &mut Window,
    cx: &mut Context<TranscriptionPage>,
) -> Div {
    let p = &c.p;
    // The paragraph still being heard lives in the lanes; only closed paragraphs are rows. The store can be ahead of
    // the status: a paragraph closed after it is still in the lanes, so it waits.
    let live = page.status.state.transcript_id() == Some(open.summary.id);
    let rows: Vec<&ParagraphView> = open
        .paragraphs
        .iter()
        .filter(|v| v.data.state != ParagraphState::Listening)
        .take(if live { page.status.closed } else { usize::MAX })
        .collect();
    let inner = if rows.is_empty() {
        let empty = if live { Tr::TranscriptionListening } else { Tr::TranscriptionNothingYet };
        div().w_full().p(px(24.)).child(text(c.t(empty), 13., FontWeight::NORMAL, p.muted)).into_any_element()
    } else {
        let last = rows.len() - 1;
        let mut list =
            div().id("transcript-detail").size_full().flex().flex_col().overflow_y_scroll().track_scroll(&page.scroll);
        for (i, v) in rows.iter().enumerate() {
            list = list.child(paragraph_row(
                c,
                v,
                open.summary.sources.both(),
                open.summary.started_ts,
                i == last,
                window,
                cx,
            ));
        }
        list.into_any_element()
    };
    let scrolled = -f32::from(page.scroll.offset().y);
    let max = f32::from(page.scroll.max_offset().y);
    let top = (scrolled / FADE).clamp(0., 1.);
    let bottom = ((max - scrolled) / FADE).clamp(0., 1.);
    ui::card(p)
        .flex_1()
        .min_h(px(0.))
        .overflow_hidden()
        .child(div().relative().size_full().child(inner))
        .when(top > 0.001, |card| card.child(edge_fade(Edge::Top, p.card, FADE, 0.).opacity(top)))
        .when(bottom > 0.001, |card| card.child(edge_fade(Edge::Bottom, p.card, FADE, 0.).opacity(bottom)))
}

/// `labelled` shows who spoke, for a session that recorded both sides.
fn paragraph_row(
    c: &Ctx,
    v: &ParagraphView,
    labelled: bool,
    started_ts: i64,
    last: bool,
    window: &Window,
    cx: &mut Context<TranscriptionPage>,
) -> Stateful<Div> {
    let p = &c.p;
    let key = ElementId::from(("transcript-paragraph-row", v.data.id as usize));
    let copy_id = ElementId::from(("transcript-paragraph-copy", v.data.id as usize));
    let paragraph_id = v.data.id;
    let hovered = ui::is_hovered(&key) || ui::is_hovered(&copy_id);
    let at = hhmm(started_ts + v.data.start_ms as i64 / 1000, dito_platform::local_utc_offset_secs());
    let mut meta = div()
        .w(px(META_COL))
        .flex_none()
        .h(px(META_H))
        .pt(px(META_TOP))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().w(px(TIME_COL)).flex_none().child(text(at, 12., FontWeight::NORMAL, p.muted)));
    if labelled {
        let (dot, ink) = if v.data.speaker == Speaker::Me { (p.primary, p.accent_text) } else { (p.muted, p.text) };
        meta = meta.child(
            div().flex().items_center().gap(px(6.)).child(div().size(px(6.)).rounded_full().bg(dot)).child(text(
                c.t(speaker_label(v.data.speaker)),
                13.,
                FontWeight::SEMIBOLD,
                ink,
            )),
        );
    }
    let body = match &v.editor {
        Some(editor) => {
            let mut column = div().flex().flex_col().gap(px(5.)).child(paragraph_editor(p, editor, window, cx));
            if v.data.error.is_some() {
                column = column.child(div().px(px(EDITOR_PAD_X)).child(text(
                    c.t(Tr::TranscriptionNotPolished),
                    12.,
                    FontWeight::NORMAL,
                    p.muted,
                )));
            }
            column
        }
        None => div().child(paragraph_polishing(c, v)),
    };
    let action = div().w(px(ACTION_COL)).h(px(ACTION_H)).pt(px(ACTION_TOP)).flex_none().child(if hovered {
        ui::copy_button_with(
            copy_id,
            p,
            CircleSize::Small,
            cbk_copy(cx, move |page, _, cx| page.copy_paragraph(paragraph_id, cx)),
        )
        .into_any_element()
    } else {
        div().into_any_element()
    });
    let mut row = div()
        .id(key.clone())
        .w_full()
        .flex_none()
        .relative()
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .flex()
        .items_start()
        .gap(px(4.))
        .on_hover(ui::hover_tracker(key.clone()));
    if hovered {
        row = row.child(ui::hover_fill(&key, Some(0.), p.sub));
    }
    row = row.child(meta).child(div().flex_1().min_w(px(0.)).child(body)).child(action);
    if !last {
        row = row.border_b_1().border_color(p.border);
    }
    row
}

/// The words of a paragraph that is still being polished, with the state that says why it is not editable yet.
fn paragraph_polishing(c: &Ctx, v: &ParagraphView) -> Div {
    let p = &c.p;
    let mut column = div().flex().flex_col().gap(px(5.)).child(paragraph_text(v.data.text.clone(), p.muted));
    if v.data.state == ParagraphState::Polishing {
        column = column.child(
            div().flex().items_center().gap(px(5.)).child(ui::icon(Ic::Sparkles, 12., p.accent_text)).child(text(
                c.t(Tr::TranscriptionPolishing),
                11.,
                FontWeight::MEDIUM,
                p.accent_text,
            )),
        );
    }
    column
}

/// A paragraph's words laid out exactly as the textarea lays them out: the same font, inset and wrap width.
fn paragraph_text(s: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .w_full()
        .pl(px(EDITOR_PAD_X))
        .pr(px(EDITOR_PAD_X + EDITOR_WRAP_MARGIN))
        .py(px(EDITOR_PAD_Y))
        .font_family(ui::cjk_family())
        .text_size(px(BODY_SIZE))
        .font_weight(FontWeight::NORMAL)
        .line_height(px(BODY_LINE))
        .text_color(color)
        .child(s.into())
}

/// Borderless until focused, so a finished transcript reads as text and is still edited in place. The height comes
/// from the same words laid out as plain text underneath, so it is right on the first frame: the textarea only knows
/// its wrapped rows after it has painted once. The plain text shows while the textarea's own text is hidden, and the
/// other way round while it has focus.
fn paragraph_editor(p: &Palette, editor: &Entity<TextareaState>, window: &Window, cx: &App) -> Div {
    let value = editor.read(cx).value();
    let focused = editor.read(cx).focus_handle(cx).contains_focused(window, cx);
    let hidden = ui::transparent();
    let body = div()
        .relative()
        .w_full()
        .rounded(px(10.))
        .child(paragraph_text(value, if focused { hidden } else { p.text }))
        .child(
            div().absolute().inset_0().child(
                Textarea::new(editor)
                    .appearance(false)
                    .bordered(false)
                    .h_full()
                    .text_size(px(BODY_SIZE))
                    .font_weight(FontWeight::NORMAL)
                    .line_height(px(BODY_LINE))
                    .when(!focused, |t| t.text_color(hidden)),
            ),
        );
    ui::with_ring(
        body,
        ("transcript-paragraph", editor.entity_id().as_u64() as usize),
        focused,
        10.,
        ui::transparent(),
        p.accent,
    )
}
