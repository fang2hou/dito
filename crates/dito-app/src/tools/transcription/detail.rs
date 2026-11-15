//! One transcript as an editable document: a fixed speaker column, paragraphs edited in place, and the whole
//! meeting's copy and delete in the header.

use dito_core::i18n::{Tr, fmt};
use dito_core::store::{ParagraphState, Speaker};
use dito_core::transcription::SessionState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{OpenTranscript, ParagraphView, TranscriptionPage, clock, date_time, row_fill, speaker_label};
use crate::assets::Ic;
use crate::settings::{Ctx, Edge, edge_fade};
use crate::theme::Palette;
use crate::timefmt::hhmm;
use crate::ui::{self, BtnKind, CircleAppearance, CircleSize, Textarea, TextareaState, cbk, cbk_copy, text};

pub(super) const BACK_BUTTON_ID: &str = "transcript-back";

/// `design:tools/detail` measurements.
const META_COL: f32 = 98.;
const TIME_COL: f32 = 36.;
const ROW_PAD_X: f32 = 20.;
const ROW_PAD_Y: f32 = 10.;
const BODY_SIZE: f32 = 14.;
const BODY_LINE: f32 = 22.;
const ACTION_COL: f32 = 32.;
/// The row's gap between columns, and the larger room kept between the editing ring and the copy button.
const ROW_GAP: f32 = 4.;
const RING_TO_ACTION: f32 = 12.;
const FADE: f32 = 24.;
/// The inset around a paragraph's words, set on both the plain text and the borderless textarea so editing never
/// moves them; the borderless textarea keeps its own 10/8 pt padding, so it takes the difference.
const EDITOR_PAD_X: f32 = 12.;
const EDITOR_PAD_Y: f32 = 9.;
/// gpui-base's textarea pads its text by this much once its border is off.
const TEXTAREA_PAD_X: f32 = 10.;
const TEXTAREA_PAD_Y: f32 = 8.;
/// The textarea wraps this far short of its text area (gpui-base's `RIGHT_MARGIN`).
const EDITOR_WRAP_MARGIN: f32 = 10.;
/// The meta column and the hover copy button hang from the first text line, which starts `EDITOR_PAD_Y` below the row.
const META_TOP: f32 = EDITOR_PAD_Y;
const META_H: f32 = BODY_LINE + EDITOR_PAD_Y;
const ACTION_TOP: f32 = (2. * EDITOR_PAD_Y + BODY_LINE - ACTION_COL) / 2.;
const ACTION_H: f32 = ACTION_COL + ACTION_TOP;

const HEADER_H: f32 = 36.;
const SECTION_GAP: f32 = 18.;
const REVIEW_H: f32 = 2. * HEADER_H + 2. * SECTION_GAP;

pub(super) fn view(
    page: &TranscriptionPage,
    c: &Ctx,
    pose: super::dock::Pose,
    window: &mut Window,
    cx: &mut Context<TranscriptionPage>,
) -> Div {
    let Some(open) = &page.open else { return div() };
    let live = page.status.state.transcript_id() == Some(open.summary.id);
    let review_height = REVIEW_H * pose.review;
    let reveal = |bottom: f32| ((review_height - bottom) / SECTION_GAP).clamp(0., 1.);
    let title = reveal(HEADER_H);
    let meta = reveal(2. * HEADER_H + SECTION_GAP);
    let header = if live { live_head(open, page, c, cx) } else { finished_head(page, c, cx) };
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(div().h(px(HEADER_H)).flex_none().flex().items_center().child(header))
        .child(
            div()
                .relative()
                .h(px(SECTION_GAP + review_height))
                .flex_none()
                .when(title > 0., |d| {
                    d.child(div().absolute().top(px(SECTION_GAP)).w_full().opacity(title).child(ui::text_input(
                        &c.p,
                        &open.title,
                        None,
                    )))
                })
                .when(meta > 0., |d| {
                    d.child(meta_strip(open, c).absolute().top(px(2. * SECTION_GAP + HEADER_H)).opacity(meta))
                }),
        )
        .child(
            div().relative().flex_1().min_h(px(0.)).child(
                paragraph_card(open, page, c, pose.entry, window, cx)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(relative(pose.entry)),
            ),
        )
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
        .child(
            div().flex().items_center().gap(px(12.)).child(back_to_list(c, cx)).child(
                ui::page_title(
                    p,
                    c.t(if matches!(page.status.state, SessionState::Finishing { .. }) {
                        Tr::TranscriptionFinishing
                    } else if page.is_paused() {
                        Tr::TranscriptionPaused
                    } else {
                        Tr::TranscriptionRecording
                    }),
                )
                .line_height(px(HEADER_H)),
            ),
        )
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
                .child(super::list::badge(p, fmt::transcription_count(c.lang, &count.to_string()))),
        )
}

/// The header of a finished transcript: the way back, copy, and delete.
fn finished_head(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    div().w_full().flex().items_center().justify_between().gap(px(24.)).child(back_to_list(c, cx)).child(
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(ui::copy_button("transcript-copy-all", p, cbk_copy(cx, |page, _, cx| page.copy_open(cx))))
            .child(delete_button(page, c, cx)),
    )
}

fn back_to_list(c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Stateful<Div> {
    let id = ElementId::from(BACK_BUTTON_ID);
    let label = c.t(Tr::TranscriptionBack);
    ui::tip_label(id.clone(), label.clone());
    ui::icon_button(id.clone(), &c.p, Ic::ArrowLeft, false, cbk(cx, |page, _, cx| page.close_detail(cx)))
        .role(Role::Button)
        .aria_label(label)
        .child(ui::tip_above(&c.p, &id))
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

fn delete_button(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Stateful<Div> {
    let p = &c.p;
    let id = "transcript-delete";
    let delete = cbk(cx, |page, _, cx| page.delete_open(cx));
    if page.delete_armed {
        let label = c.t(Tr::TranscriptionDeleteConfirm);
        return ui::button(id, p, &label, BtnKind::Danger, Some(Ic::Trash), delete)
            .role(Role::Button)
            .aria_label(label);
    }
    ui::circle_control(
        id,
        Ic::Trash,
        CircleSize::Regular,
        CircleAppearance {
            background: Some(p.button_danger_bg),
            foreground: p.button_danger_text,
            border: Some(p.button_danger_border),
            hover: Some(p.button_danger_hover),
        },
    )
    .cursor_pointer()
    .on_click(ui::click_cb(delete))
    .role(Role::Button)
    .aria_label(c.t(Tr::TranscriptionDelete))
}

fn paragraph_card(
    open: &OpenTranscript,
    page: &TranscriptionPage,
    c: &Ctx,
    entry: f32,
    window: &mut Window,
    cx: &mut Context<TranscriptionPage>,
) -> Stateful<Div> {
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
    let visible = ((entry - 0.16) / 0.14).clamp(0., 1.);
    let inner = if rows.is_empty() {
        let empty = if live { Tr::TranscriptionListening } else { Tr::TranscriptionNothingYet };
        div().w_full().p(px(24.)).child(text(c.t(empty), 13., FontWeight::NORMAL, p.muted)).into_any_element()
    } else {
        let last = rows.len() - 1;
        let mut list =
            div().id("transcript-detail").size_full().flex().flex_col().overflow_y_scroll().track_scroll(&page.scroll);
        for (i, v) in rows.iter().enumerate() {
            list = list.child(paragraph_row(c, v, &open.summary, i == last, window, cx, &page.scroll));
        }
        list.into_any_element()
    };
    let scrolled = -f32::from(page.scroll.offset().y);
    let max = f32::from(page.scroll.max_offset().y);
    let top = (scrolled / FADE).clamp(0., 1.);
    let bottom = ((max - scrolled) / FADE).clamp(0., 1.);
    let card = ui::card(p)
        .size_full()
        .overflow_hidden()
        .child(div().relative().size_full().opacity(visible).child(inner))
        .when(top > 0.001, |card| card.child(edge_fade(Edge::Top, p.card, FADE, 0.).opacity(top)))
        .when(bottom > 0.001, |card| card.child(edge_fade(Edge::Bottom, p.card, FADE, 0.).opacity(bottom)));
    // The thumb hangs outside the clipped card, 6 pt to its right.
    let area = ElementId::from("transcript-detail-area");
    let extent = ui::ScrollExtent { viewport: f32::from(page.scroll.bounds().size.height), max, scrolled };
    div()
        .id(area.clone())
        .relative()
        .flex_1()
        .min_h(px(0.))
        .on_hover(ui::hover_tracker(area.clone()))
        .child(card)
        .child(ui::overlay_thumb("transcript-detail-thumb", p, ui::is_hovered(&area), extent, 0.))
}

/// Show the speaker when the session recorded both sides.
fn paragraph_row(
    c: &Ctx,
    v: &ParagraphView,
    summary: &dito_core::store::TranscriptSummary,
    last: bool,
    window: &Window,
    cx: &mut Context<TranscriptionPage>,
    scroll: &ScrollHandle,
) -> Stateful<Div> {
    let p = &c.p;
    let key = ElementId::from(("transcript-paragraph-row", v.data.id as usize));
    let copy_id = ElementId::from(("transcript-paragraph-copy", v.data.id as usize));
    let paragraph_id = v.data.id;
    let hovered = ui::is_hovered(&key) || ui::is_hovered(&copy_id);
    let at = hhmm(summary.started_ts + v.data.start_ms as i64 / 1000, dito_platform::local_utc_offset_secs());
    let mut meta = div()
        .w(px(META_COL))
        .flex_none()
        .h(px(META_H))
        .pt(px(META_TOP))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().w(px(TIME_COL)).flex_none().child(text(at, 12., FontWeight::NORMAL, p.muted)));
    if summary.sources.both() {
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
        .gap(px(ROW_GAP))
        .on_hover(ui::hover_tracker(key.clone()));
    let scroll = scroll.clone();
    let color = p.sub;
    row = row
        .child(ui::tweened(ElementId::from((key, "row-fill")), f32::from(hovered), ui::HOVER_MS, move |opacity| {
            row_fill(scroll.clone(), color.opacity(opacity))
        }))
        .child(meta)
        // The editing ring ends 12 pt before the copy button rather than touching the column.
        .child(div().flex_1().min_w(px(0.)).mr(px(RING_TO_ACTION - ROW_GAP)).child(body))
        .child(action);
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
                    .px(px(EDITOR_PAD_X - TEXTAREA_PAD_X))
                    .py(px(EDITOR_PAD_Y - TEXTAREA_PAD_Y))
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

impl TranscriptionPage {
    /// Focuses the first editable paragraph of the open transcript, for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub fn pose_focus_paragraph(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.open.as_ref().and_then(|open| open.paragraphs.iter().find_map(|v| v.editor.clone()))
        else {
            return;
        };
        editor.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }
}
