//! The meeting list of `design:tools/transcripts`: search and period filters, then every saved transcript grouped
//! by day, newest first, with the running session on top. The list is virtualized per day, scrolls under the
//! floating Dock Panel, and shows an overlay thumb while the pointer is over it.

use std::cell::RefCell;

use dito_core::i18n::{Lang, Tr};
use dito_core::store::TranscriptSummary;
use dito_core::transcription::SessionState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TranscriptionPage, open_row};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::timefmt::{day_start, hhmm, local_day, now, ymd};
use crate::ui::{self, HOVER_MS, InputEvent, InputState, cbk_idx, text};

/// `design:tools/transcripts` measurements.
const ROW_PAD_X: f32 = 22.;
const ROW_PAD_Y: f32 = 15.;
/// On hover the arrow slides this far right inside its own slot, so the time beside it never moves.
const ARROW_SHIFT: f32 = 3.;
const ARROW_MS: u64 = 220;
const ARROW_SIZE: f32 = 16.;
const ROW_RADIUS: f32 = 15.;
const TOOLBAR_GAP: f32 = 12.;
const HEAD_ABOVE: f32 = 20.;
const HEAD_BELOW: f32 = 12.;
/// The last row can always scroll this far above the floating Dock Panel.
const CLEARANCE: f32 = 24.;
/// A generated title's emoji sits this much lower than the title's line box, on its optical center.
const EMOJI_DROP: f32 = 1.;
const EMOJI_GAP: f32 = 6.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Period {
    All,
    Today,
    Week,
}

/// The list's own state: the search field, the period, and the virtual list's scroll.
pub(super) struct Browse {
    search: Entity<InputState>,
    query: String,
    period: Period,
    state: ListState,
    listed: usize,
    placeholder_lang: Lang,
}

impl Browse {
    pub(super) fn new(lang: Lang, window: &mut Window, cx: &mut Context<TranscriptionPage>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::TranscriptionSearch)));
        ui::notify_on_focus(&search, window, cx);
        cx.subscribe_in(&search, window, |page: &mut TranscriptionPage, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                page.browse.query = state.read(cx).value().to_string();
                page.browse.state.scroll_to(ListOffset::default());
                cx.notify();
            }
        })
        .detach();
        let state = ListState::new(0, ListAlignment::Top, px(600.));
        // The list scrolls without re-running the page view, so the thumb would freeze; nudge the page weakly.
        let page = cx.entity().downgrade();
        state.set_scroll_handler(move |_, _, cx| {
            page.update(cx, |_, cx| cx.notify()).ok();
        });
        Self { search, query: String::new(), period: Period::All, state, listed: 0, placeholder_lang: lang }
    }
}

/// The toolbar and the grouped list. `bottom` is the room the floating Dock Panel takes at the bottom.
pub(super) fn view(
    page: &mut TranscriptionPage,
    c: &Ctx,
    bottom: f32,
    window: &mut Window,
    cx: &mut Context<TranscriptionPage>,
) -> Div {
    let p = &c.p;
    if page.browse.placeholder_lang != c.lang {
        page.browse.placeholder_lang = c.lang;
        let placeholder = c.t(Tr::TranscriptionSearch);
        page.browse.search.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
    }
    let live = live_id(page);
    let rows = filtered(&page.list, live, &page.browse.query, page.browse.period);
    let toolbar = toolbar(page, c, cx);
    let body = if page.list.is_empty() && live.is_none() {
        empty(p, c.t(Tr::TranscriptionEmpty)).into_any_element()
    } else if rows.is_empty() {
        empty(p, c.t(Tr::TranscriptionNoMatches)).into_any_element()
    } else {
        grouped(page, c, cx, &rows, live, bottom).into_any_element()
    };
    div().size_full().flex().flex_col().gap(px(18.)).child(toolbar).child(div().flex_1().min_h(px(0.)).child(body))
}

fn toolbar(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    let period = ui::seg(
        "transcript-period",
        p,
        &[c.t(Tr::HistoryAll), c.t(Tr::HistoryToday), c.t(Tr::HistoryThisWeek)],
        match page.browse.period {
            Period::All => 0,
            Period::Today => 1,
            Period::Week => 2,
        },
        cbk_idx(cx, |page: &mut TranscriptionPage, i, _, cx| {
            page.browse.period = [Period::All, Period::Today, Period::Week][i];
            page.browse.state.scroll_to(ListOffset::default());
            cx.notify();
        }),
    );
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(TOOLBAR_GAP))
        .child(div().flex_1().min_w(px(0.)).child(ui::text_input(p, &page.browse.search, Some(Ic::Search))))
        .child(period)
}

fn empty(p: &Palette, message: SharedString) -> Div {
    ui::card(p).p(px(24.)).child(text(message, 13., FontWeight::NORMAL, p.muted).line_height(px(20.)))
}

/// The transcript a running session records, while it records.
fn live_id(page: &TranscriptionPage) -> Option<i64> {
    match page.status.state {
        SessionState::Recording { id, .. } => Some(id),
        _ => None,
    }
}

/// The running session leads, the rest stay newest first; the query matches the title or the first words.
fn filtered(list: &[TranscriptSummary], live: Option<i64>, query: &str, period: Period) -> Vec<TranscriptSummary> {
    let off = dito_platform::local_utc_offset_secs();
    let since = match period {
        Period::All => None,
        Period::Today => Some(day_start(local_day(now(), off), off)),
        Period::Week => Some(now() - 7 * 86_400),
    };
    let needle = query.trim().to_lowercase();
    let mut rows: Vec<TranscriptSummary> = list
        .iter()
        .filter(|row| since.is_none_or(|since| row.started_ts >= since))
        .filter(|row| {
            needle.is_empty()
                || row.title.to_lowercase().contains(&needle)
                || row.preview.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
    if let Some(live) = live
        && let Some(at) = rows.iter().position(|row| row.id == live)
    {
        let row = rows.remove(at);
        rows.insert(0, row);
    }
    rows
}

/// One heading and one card per day, virtualized per day, with the overlay thumb beside it.
fn grouped(
    page: &mut TranscriptionPage,
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    rows: &[TranscriptSummary],
    live: Option<i64>,
    bottom: f32,
) -> Stateful<Div> {
    let p = &c.p;
    let off = dito_platform::local_utc_offset_secs();
    let today = local_day(now(), off);
    let mut items: Vec<Option<AnyElement>> = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        // The live meeting belongs to today whatever its start, so it never opens a group of its own.
        let day_of =
            |row: &TranscriptSummary| if live == Some(row.id) { today } else { local_day(row.started_ts, off) };
        let day = day_of(&rows[i]);
        let end = rows[i..].iter().position(|row| day_of(row) != day).map_or(rows.len(), |n| i + n);
        let heading = if day == today {
            c.t(Tr::HistoryToday).to_string()
        } else if day == today - 1 {
            c.t(Tr::HistoryYesterday).to_string()
        } else {
            ymd(day).replace('-', "/")
        };
        items.push(Some(
            div()
                .w_full()
                .pt(px(if items.is_empty() { 0. } else { HEAD_ABOVE }))
                .pb(px(HEAD_BELOW))
                .child(text(heading, 16., FontWeight::BOLD, p.text))
                .into_any_element(),
        ));
        let count = end - i;
        let mut card = ui::card(p).flex().flex_col();
        for (k, row) in rows[i..end].iter().enumerate() {
            card = card.child(row_view(c, cx, row, live, k == 0, k + 1 == count));
            if k + 1 < count {
                card = card.child(div().w_full().h(px(1.)).flex_none().bg(p.border));
            }
        }
        items.push(Some(card.into_any_element()));
        i = end;
    }
    items.push(Some(div().h(px(bottom + CLEARANCE)).into_any_element()));
    let n = items.len();
    if n != page.browse.listed {
        page.browse.listed = n;
        page.browse.state.reset(n);
    }
    let items = RefCell::new(items);
    let list = list(page.browse.state.clone(), move |ix, _, _| {
        items.borrow_mut().get_mut(ix).and_then(Option::take).unwrap_or_else(|| div().into_any_element())
    })
    .size_full();
    let hover_id = ElementId::from("transcript-list-area");
    let hovered = ui::is_hovered(&hover_id);
    div()
        .id(hover_id.clone())
        .relative()
        .size_full()
        .on_hover(ui::hover_tracker(hover_id.clone()))
        .child(list)
        .child(ui::overlay_thumb("transcript-list-thumb", p, hovered, extent(&page.browse.state), bottom + CLEARANCE))
}

/// The virtual list's extent for the overlay thumb.
fn extent(state: &ListState) -> ui::ScrollExtent {
    ui::ScrollExtent {
        viewport: f32::from(state.viewport_bounds().size.height),
        max: f32::from(state.max_offset_for_scrollbar().y),
        scrolled: -f32::from(state.scroll_px_offset_for_scrollbar().y),
    }
}

fn row_view(
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    row: &TranscriptSummary,
    live: Option<i64>,
    first: bool,
    last: bool,
) -> Stateful<Div> {
    let p = c.p;
    let id = row.id;
    let key = ElementId::from(("transcript-row", id as usize));
    let running = live == Some(id);
    let hovered = ui::is_hovered(&key);
    let mut head = div().flex().items_center().gap(px(10.)).child(title(c, &row.title));
    if running {
        head = head.child(
            div().flex().items_center().gap(px(5.)).child(div().size(px(6.)).rounded_full().bg(p.danger)).child(text(
                c.t(Tr::TranscriptionRecording),
                12.,
                FontWeight::MEDIUM,
                p.danger_text,
            )),
        );
    } else if let Some(length) = row.length_ms.filter(|_| row.ended_ts.is_some()) {
        head = head.child(badge(&p, crate::timefmt::clock(length)));
    }
    let mut column = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(4.)).child(head);
    if !row.preview.is_empty() {
        column = column.child(text(row.preview.clone(), 13., FontWeight::NORMAL, p.muted).truncate());
    }
    let arrow = div().w(px(ARROW_SIZE + ARROW_SHIFT)).h(px(ARROW_SIZE)).flex_none().relative().child(ui::tweened(
        ElementId::from((key.clone(), "arrow")),
        if hovered { ARROW_SHIFT } else { 0. },
        ARROW_MS,
        move |shift| {
            div().absolute().top_0().left(px(shift)).child(ui::icon(
                Ic::ArrowRight,
                ARROW_SIZE,
                ui::mix_rgb(p.muted, p.text, shift / ARROW_SHIFT),
            ))
        },
    ));
    let at = hhmm(row.started_ts, dito_platform::local_utc_offset_secs());
    let tail = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(text(at, 13., FontWeight::NORMAL, p.muted))
        .child(arrow);
    let fill = if running { p.accent_soft } else { p.sub };
    let (top, bottom) = (if first { ROW_RADIUS } else { 0. }, if last { ROW_RADIUS } else { 0. });
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
            move |opacity| {
                div()
                    .absolute()
                    .inset_0()
                    .rounded_tl(px(top))
                    .rounded_tr(px(top))
                    .rounded_bl(px(bottom))
                    .rounded_br(px(bottom))
                    .bg(fill.opacity(opacity))
            },
        ))
        .child(column)
        .child(tail)
}

/// The row title. A generated title leads with its emoji, drawn as its own glyph and dropped 1 pt so it sits on
/// the title's optical center (`design:tools/transcripts`).
fn title(c: &Ctx, raw: &str) -> Div {
    let p = &c.p;
    let raw = raw.trim();
    if raw.is_empty() {
        return text(c.t(Tr::TranscriptionUntitled), 16., FontWeight::SEMIBOLD, p.text);
    }
    match split_emoji(raw) {
        Some((emoji, rest)) => div()
            .flex()
            .items_center()
            .gap(px(EMOJI_GAP))
            .child(text(emoji.to_string(), 16., FontWeight::SEMIBOLD, p.text).relative().top(px(EMOJI_DROP)))
            .child(text(rest.to_string(), 16., FontWeight::SEMIBOLD, p.text)),
        None => text(raw.to_string(), 16., FontWeight::SEMIBOLD, p.text),
    }
}

/// A title that starts with an emoji followed by a space, split into the two; anything else stays whole.
fn split_emoji(title: &str) -> Option<(&str, &str)> {
    let (head, rest) = title.split_once(' ')?;
    let first = head.chars().next()?;
    let pictographic = matches!(u32::from(first), 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF);
    (pictographic && !rest.trim().is_empty()).then(|| (head, rest.trim_start()))
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

#[cfg(test)]
mod tests {
    use super::split_emoji;

    #[test]
    fn a_leading_emoji_splits_off_and_plain_titles_stay_whole() {
        assert_eq!(split_emoji("📋 Release plan"), Some(("📋", "Release plan")));
        assert_eq!(split_emoji("Weekly sync"), None);
        assert_eq!(split_emoji("1on1 review"), None);
        assert_eq!(split_emoji("📋 "), None);
    }
}
