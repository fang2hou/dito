use std::sync::Arc;

use dito_core::i18n::{Lang, Tr, fmt};
use dito_core::store::{Period, Stats, civil, heat_quartiles, heat_step, week_start};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::edge_fade::{Edge, edge_fade};
use super::page_query::{PageQuery, QueryPage};
use super::{Ctx, content_width};
use crate::services::{self, StatsSnapshot};
use crate::state::AppState;
use crate::theme::Palette;
use crate::timefmt::{local_day, now};
use crate::ui::{self, cbk_idx, text};

const PERIODS: [Period; 3] = [Period::Week, Period::Month, Period::All];

/// Monday first, as the weeks of the daily view are.
const WEEKDAYS: [Tr; 7] = [
    Tr::StatsWeekdayMon,
    Tr::StatsWeekdayTue,
    Tr::StatsWeekdayWed,
    Tr::StatsWeekdayThu,
    Tr::StatsWeekdayFri,
    Tr::StatsWeekdaySat,
    Tr::StatsWeekdaySun,
];

const MONTHS: [Tr; 12] = [
    Tr::StatsMonthJan,
    Tr::StatsMonthFeb,
    Tr::StatsMonthMar,
    Tr::StatsMonthApr,
    Tr::StatsMonthMay,
    Tr::StatsMonthJun,
    Tr::StatsMonthJul,
    Tr::StatsMonthAug,
    Tr::StatsMonthSep,
    Tr::StatsMonthOct,
    Tr::StatsMonthNov,
    Tr::StatsMonthDec,
];

const HEAT_CELL: f32 = 10.;
const HEAT_PITCH: f32 = 13.;
/// Opacity of the languages the pointer is not on.
const LANGUAGE_DIMMED: f32 = 0.35;
/// Both cards of the bottom row, whatever they hold.
const BOTTOM_CARD_H: f32 = 273.;
/// Three and a half app rows, so the cut row invites scrolling.
const APPS_VIEWPORT_H: f32 = 177.;
const APP_ROW_H: f32 = 35.;
const APP_ROW_GAP: f32 = 18.;
const APPS_FADE: f32 = 24.;

/// The Stats page, `design:settings`.
pub struct StatsPage {
    st: Arc<AppState>,
    period: Period,
    /// Owned snapshot; render never touches the database.
    query: PageQuery<StatsSnapshot>,
    apps_scroll: ScrollHandle,
    motion: ui::MotionScope,
}

impl QueryPage for StatsPage {
    type Data = StatsSnapshot;

    fn page_query(&mut self) -> &mut PageQuery<StatsSnapshot> {
        &mut self.query
    }

    fn fetch(&self) -> impl Future<Output = anyhow::Result<StatsSnapshot>> + 'static {
        let off = dito_platform::local_utc_offset_secs();
        services::stats_snapshot(self.st.clone(), self.period, local_day(now(), off), off)
    }
}

impl StatsPage {
    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let today = local_day(now(), dito_platform::local_utc_offset_secs());
        Self {
            motion: ui::MotionScope::new(),
            st,
            period: Period::Week,
            query: PageQuery::new("stats", StatsSnapshot::empty(Period::Week, today)),
            apps_scroll: ScrollHandle::new(),
        }
    }

    /// While inactive no queries run and late results are ignored.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.query.set_active(active);
        if active {
            self.load(cx);
        }
    }

    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for StatsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Own scope: after a page switch the controls mount statically instead of sliding to values remembered from an
        // earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(_window, cx))
    }
}

impl StatsPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let snapshot = &self.query.data;
        let avail = content_width(window);
        let period_seg = ui::seg(
            "stats-range",
            &p,
            &[c.t(Tr::HistoryThisWeek), c.t(Tr::HistoryThisMonth), c.t(Tr::HistoryAll)],
            PERIODS.iter().position(|period| *period == self.period).unwrap_or(0),
            cbk_idx(cx, |this: &mut Self, i, _, cx| {
                this.period = PERIODS[i];
                this.apps_scroll.set_offset(point(px(0.), px(0.)));
                this.changed(cx);
                cx.notify();
            }),
        );
        // The list scrolls without re-running this view; the fades follow only if it is asked to.
        let apps_scrolled = cx.listener(|_: &mut Self, _: &ScrollEvent, _, cx| cx.notify());

        super::page(
            p,
            "stats",
            c.t(Tr::NavStats),
            Some(period_seg.into_any_element()),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(28.))
                .child(summary_card(&c, &snapshot.stats, avail))
                .child(daily_card(&c, snapshot, avail))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_start()
                        .gap(px(20.))
                        .child(top_apps_card(&c, &snapshot.stats, &self.apps_scroll, apps_scrolled))
                        .child(languages_card(&c, &snapshot.stats)),
                ),
        )
    }
}

/// `motion/hover`, or the short fade that replaces it under Reduce Motion.
fn hover_ms() -> u64 {
    if dito_platform::system::reduce_motion() { 80 } else { ui::HOVER_MS }
}

fn lang_name(code: &str, lang: Lang) -> String {
    if code.is_empty() {
        return "—".into();
    }
    Lang::from_tag(code).map_or(code, |language| language.name(lang)).into()
}

fn summary_card(c: &Ctx, stats: &Stats, avail: f32) -> Div {
    let p = c.p;
    let hours = stats.saved_minutes / 60.0;
    let saved = if hours >= 10.0 {
        format!("{hours:.0} {}", c.t(Tr::UnitsHours))
    } else if hours >= 1.0 {
        format!("{hours:.1} {}", c.t(Tr::UnitsHours))
    } else {
        format!("{:.0} {}", stats.saved_minutes, c.t(Tr::UnitsMinutes))
    };
    let rest = [
        (c.t(Tr::StatsCharacters), format!("{} {}", fmt_int(stats.chars), c.t(Tr::UnitsChars))),
        (c.t(Tr::StatsRecordings), format!("{} {}", fmt_int(stats.recordings), c.t(Tr::UnitsTimes))),
        (c.t(Tr::StatsAverageSpeed), format!("{:.0} {}", stats.avg_cpm, c.t(Tr::UnitsCharsPerMin))),
    ];
    // The lead column gives way before a long number or unit would leave its cell.
    let rest_width =
        rest.iter().map(|(_, value)| ui::text_width(value, 20., FontWeight::MEDIUM) + 37.).fold(0., f32::max);
    let lead_width =
        (avail - 2. - rest_width * 3.).min(300.).max(ui::text_width(&saved, 40., FontWeight::MEDIUM) + 52.);
    // One label line and one baseline across the card: the 40 pt and 20 pt values end 22 and 26 pt above the edge.
    let label = |s: SharedString| text(s, 13., FontWeight::NORMAL, p.muted).line_height(px(21.));
    let mut card = ui::card(&p).flex_row().h(px(123.)).child(
        div()
            .flex_none()
            .w(px(lead_width))
            .h_full()
            .px(px(26.))
            .py(px(22.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(label(c.t(Tr::StatsTimeSaved)))
            .child(text(saved, 40., FontWeight::MEDIUM, p.text).line_height(px(52.)).whitespace_nowrap()),
    );
    for (name, value) in rest {
        card = card.child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .px(px(18.))
                .pt(px(22.))
                .pb(px(26.))
                .flex()
                .flex_col()
                .justify_between()
                .border_l_1()
                .border_color(p.border)
                .child(label(name))
                .child(text(value, 20., FontWeight::MEDIUM, p.text).line_height(px(30.)).whitespace_nowrap()),
        );
    }
    card
}

#[derive(Clone, Copy)]
enum DayCell {
    /// Not part of the shown range; drawn empty.
    Outside,
    Upcoming,
    Day {
        day: i64,
        chars: u64,
    },
}

/// What the daily view needs to draw and colour one day.
struct Daily<'a> {
    c: &'a Ctx,
    snapshot: &'a StatsSnapshot,
    quartiles: [u64; 3],
}

impl Daily<'_> {
    fn last_day(&self) -> i64 {
        self.snapshot.first_day + self.snapshot.days.len() as i64 - 1
    }

    fn at(&self, day: i64) -> DayCell {
        let snapshot = self.snapshot;
        let chars = usize::try_from(day - snapshot.first_day).ok().and_then(|i| snapshot.days.get(i));
        match chars {
            None => DayCell::Outside,
            Some(_) if day > snapshot.today => DayCell::Upcoming,
            Some(chars) => DayCell::Day { day, chars: *chars },
        }
    }

    /// `base` carries the size. Hover fades in the outline and asks for the shared tooltip.
    fn cell(&self, base: Div, radius: f32, day: i64) -> AnyElement {
        let p = self.c.p;
        let base = base.relative().rounded(px(radius));
        match self.at(day) {
            DayCell::Outside => base.into_any_element(),
            DayCell::Upcoming => base.bg(p.sub).border_1().border_color(p.border).into_any_element(),
            DayCell::Day { day, chars } => {
                let id = ElementId::Name(format!("stats-day-{:?}-{day}", self.snapshot.period).into());
                let tip = fmt::stats_day_tip(self.c.lang, &fmt_int(chars), &date_label(self.c, day));
                ui::tip_label(id.clone(), tip.into());
                let outline = p.text;
                base.id(id.clone())
                    .bg(heat(&p, heat_step(chars, &self.quartiles)))
                    .on_hover(ui::hover_tracker(id.clone()))
                    .child(ui::tweened(
                        ElementId::from((id.clone(), "outline")),
                        f32::from(ui::is_hovered(&id)),
                        hover_ms(),
                        move |v| {
                            div()
                                .absolute()
                                .inset_0()
                                .rounded(px(radius))
                                .border(px(1.5))
                                .border_color(ui::hex_alpha(outline, v))
                        },
                    ))
                    .child(ui::tip_above(&p, &id))
                    .into_any_element()
            }
        }
    }
}

fn heat(p: &Palette, step: usize) -> Hsla {
    [p.stats_heat_0, p.stats_heat_1, p.stats_heat_2, p.stats_heat_3, p.stats_heat_4][step.min(4)]
}

fn month_label(c: &Ctx, month: u8) -> &'static str {
    c.lang.tr(MONTHS[usize::from(month).clamp(1, 12) - 1])
}

fn date_label(c: &Ctx, day: i64) -> String {
    let (_, month, day) = civil(day);
    fmt::stats_date(c.lang, &day.to_string(), month_label(c, month))
}

fn year_month_label(c: &Ctx, day: i64) -> String {
    let (year, month, _) = civil(day);
    fmt::stats_year_month(c.lang, month_label(c, month), &year.to_string())
}

fn range_label(daily: &Daily) -> String {
    let (c, first, last) = (daily.c, daily.snapshot.first_day, daily.last_day());
    match daily.snapshot.period {
        Period::Week => {
            let ((_, from_month, from), (_, to_month, to)) = (civil(first), civil(last));
            if from_month == to_month {
                fmt::stats_days_in_month(c.lang, &from.to_string(), month_label(c, from_month), &to.to_string())
            } else {
                fmt::stats_date_span(c.lang, &date_label(c, first), &date_label(c, last))
            }
        }
        Period::Month => year_month_label(c, first),
        Period::All => fmt::stats_since(c.lang, &year_month_label(c, first)),
    }
}

fn daily_card(c: &Ctx, snapshot: &StatsSnapshot, avail: f32) -> Div {
    let p = c.p;
    let daily = Daily { c, snapshot, quartiles: heat_quartiles(snapshot.days.iter().copied()) };
    let view = match snapshot.period {
        Period::Week => week_tiles(&daily),
        Period::Month => month_calendar(&daily),
        Period::All => heatmap(&daily, avail - 54.),
    };
    let active_days = snapshot.days.iter().filter(|chars| **chars > 0).count() as u64;
    let max = snapshot.days.iter().copied().max().unwrap_or(0);
    let mut legend = div().flex_none().flex().items_center().gap(px(4.)).child(text(
        c.t(Tr::StatsLess),
        11.,
        FontWeight::NORMAL,
        p.muted,
    ));
    for step in 0..5 {
        legend = legend.child(div().flex_none().size(px(HEAT_CELL)).rounded(px(2.5)).bg(heat(&p, step)));
    }
    ui::card(&p)
        .px(px(26.))
        .py(px(22.))
        .gap(px(16.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(div().flex_1().child(text(c.t(Tr::StatsCharactersPerDay), 15., FontWeight::BOLD, p.text)))
                .child(ui::muted12(&p, range_label(&daily)).flex_none()),
        )
        .child(view)
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(16.))
                .child(ui::muted12(&p, fmt::stats_active_days(c.lang, &fmt_int(active_days))).flex_none())
                .child(div().flex_1().child(ui::muted12(&p, fmt::stats_max_day(c.lang, &fmt_int(max)))))
                .child(legend.child(text(c.t(Tr::StatsMore), 11., FontWeight::NORMAL, p.muted))),
        )
}

fn week_tiles(daily: &Daily) -> Div {
    let (c, p) = (daily.c, daily.c.p);
    let mut row = div().w_full().flex().gap(px(10.));
    for (i, weekday) in WEEKDAYS.into_iter().enumerate() {
        let day = daily.snapshot.first_day + i as i64;
        let chars = match daily.at(day) {
            DayCell::Day { chars, .. } => {
                text(fmt_int(chars), 13., FontWeight::MEDIUM, if chars > 0 { p.text } else { p.muted })
            }
            DayCell::Outside | DayCell::Upcoming => div(),
        };
        let (weight, color) =
            if day == daily.snapshot.today { (FontWeight::MEDIUM, p.text) } else { (FontWeight::NORMAL, p.muted) };
        row = row.child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(daily.cell(div().w_full().h(px(56.)), 10., day))
                .child(chars.h(px(18.)).line_height(px(18.)))
                .child(text(c.t(weekday), 12., weight, color).line_height(px(16.))),
        );
    }
    row
}

fn month_calendar(daily: &Daily) -> Div {
    let (c, p) = (daily.c, daily.c.p);
    let small = |s: SharedString| text(s, 11., FontWeight::NORMAL, p.muted).line_height(px(14.));
    let mut weekdays = div().w_full().pl(px(46.)).flex().gap(px(6.));
    for weekday in WEEKDAYS {
        weekdays = weekdays.child(div().flex_1().flex().justify_center().child(small(c.t(weekday))));
    }
    let mut calendar = div().w_full().flex().flex_col().gap(px(6.)).child(weekdays);
    let mut monday = week_start(daily.snapshot.first_day);
    while monday <= daily.last_day() {
        let (_, month, day) = civil(monday);
        let mut week = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().flex_none().w(px(40.)).child(small(format!("{month}/{day}").into())));
        for day in monday..monday + 7 {
            week = week.child(daily.cell(div().flex_1().h(px(26.)), 6., day));
        }
        calendar = calendar.child(week);
        monday += 7;
    }
    calendar
}

/// One column per week, Monday on top. Days still to come are left out, so the last column ends at today.
fn heatmap(daily: &Daily, avail: f32) -> Div {
    let (c, p) = (daily.c, daily.c.p);
    let small = |s: SharedString| text(s, 11., FontWeight::NORMAL, p.muted).line_height(px(14.)).whitespace_nowrap();
    let labelled = [0usize, 2, 4];
    let labels_width = labelled
        .iter()
        .map(|row| ui::text_width(c.lang.tr(WEEKDAYS[*row]), 11., FontWeight::NORMAL))
        .fold(24., f32::max);
    let weeks = daily.snapshot.days.len() / 7;
    let fitting = ((avail - labels_width - 10. + HEAT_PITCH - HEAT_CELL) / HEAT_PITCH).floor().max(1.) as usize;
    let skipped = weeks.saturating_sub(fitting);

    let mut weekday_labels = div().relative().flex_none().w(px(labels_width)).h(px(HEAT_PITCH * 7. - 3.));
    for row in labelled {
        weekday_labels =
            weekday_labels.child(small(c.t(WEEKDAYS[row])).absolute().left_0().top(px(row as f32 * HEAT_PITCH - 2.)));
    }
    let mut month_labels = div().relative().w_full().h(px(14.));
    let mut grid = div().flex().gap(px(HEAT_PITCH - HEAT_CELL));
    for (column, week) in (skipped..weeks).enumerate() {
        let monday = daily.snapshot.first_day + week as i64 * 7;
        if let Some(first_of_month) = (monday..monday + 7).find(|day| civil(*day).2 == 1) {
            let label = month_label(c, civil(first_of_month).1);
            month_labels =
                month_labels.child(small(label.into()).absolute().top_0().left(px(column as f32 * HEAT_PITCH)));
        }
        let mut days = div().flex_none().w(px(HEAT_CELL)).flex().flex_col().gap(px(HEAT_PITCH - HEAT_CELL));
        for day in (monday..monday + 7).take_while(|day| *day <= daily.snapshot.today) {
            days = days.child(daily.cell(div().flex_none().size(px(HEAT_CELL)), 2.5, day));
        }
        grid = grid.child(days);
    }
    div()
        .w_full()
        .flex()
        .items_end()
        .gap(px(10.))
        .child(weekday_labels)
        .child(div().flex_none().flex().flex_col().gap(px(6.)).child(month_labels).child(grid))
}

/// The list scrolls inside the card, `design:settings` 20.07.5.
fn top_apps_card(
    c: &Ctx,
    stats: &Stats,
    scroll: &ScrollHandle,
    on_scroll: impl Fn(&ScrollEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let p = c.p;
    let total_apps: u64 = stats.apps.iter().map(|(_, n)| *n).sum::<u64>().max(1);
    let card = ui::card(&p)
        .flex_1()
        .h(px(BOTTOM_CARD_H))
        .p(px(26.))
        .gap(px(18.))
        .child(text(c.t(Tr::StatsTopApps), 15., FontWeight::BOLD, p.text).flex_none().line_height(px(24.)));
    if stats.apps.is_empty() {
        return card.child(ui::muted12(&p, c.t(Tr::StatsNoData)));
    }
    let mut list = div()
        .id("stats-apps")
        .size_full()
        .flex()
        .flex_col()
        .gap(px(APP_ROW_GAP))
        .overflow_y_scroll()
        .track_scroll(scroll);
    for (i, (name, n)) in stats.apps.iter().enumerate() {
        let pct = *n as f32 / total_apps as f32;
        list = list.child(
            div()
                .flex_none()
                .w_full()
                .h(px(APP_ROW_H))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .w_full()
                        .h(px(19.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(text(
                            if name.is_empty() { "—".to_string() } else { name.clone() },
                            13.,
                            FontWeight::MEDIUM,
                            p.text,
                        ))
                        .child(ui::muted12(&p, format!("{:.0}%", pct * 100.))),
                )
                .child(div().w_full().h(px(8.)).rounded(px(4.)).bg(p.seg).child(
                    div().h_full().w(relative(pct)).rounded(px(4.)).bg(if i == 0 {
                        p.accent
                    } else if p.dark {
                        crate::theme::hex("#C2A765")
                    } else {
                        crate::theme::hex("#E0C47D")
                    }),
                )),
        );
    }
    // Rows have a fixed height, so the scroll range is known before the first layout.
    let rows = stats.apps.len() as f32;
    let max_scroll = (rows * (APP_ROW_H + APP_ROW_GAP) - APP_ROW_GAP - APPS_VIEWPORT_H).max(0.);
    let scrolled = (-f32::from(scroll.offset().y)).clamp(0., max_scroll);
    let top = (scrolled / APPS_FADE).clamp(0., 1.);
    let bottom = ((max_scroll - scrolled) / APPS_FADE).clamp(0., 1.);
    card.child(
        div()
            .relative()
            .flex_none()
            .w_full()
            .h(px(APPS_VIEWPORT_H))
            .child(list.on_scroll(on_scroll))
            .when(top > 0.001, |d| d.child(edge_fade(Edge::Top, p.card, APPS_FADE, 0.).opacity(top)))
            .when(bottom > 0.001, |d| d.child(edge_fade(Edge::Bottom, p.card, APPS_FADE, 0.).opacity(bottom))),
    )
}

fn languages_card(c: &Ctx, stats: &Stats) -> Div {
    let p = c.p;
    let total_langs: u64 = stats.langs.iter().map(|(_, n)| *n).sum::<u64>().max(1);
    let shades = if p.dark { ["#E0C47D", "#C2A765", "#826F41"] } else { ["#685218", "#C2A765", "#F6DA97"] };
    let shown = stats.langs.len().min(3);
    let ids: Vec<ElementId> = (0..shown).map(|i| ElementId::Name(format!("stats-language-{i}").into())).collect();
    let pointed = ids.iter().position(ui::is_hovered);
    let ms = hover_ms();
    let mut stack = div().w_full().h(px(14.)).flex();
    let mut card = ui::card(&p)
        .flex_1()
        .h(px(BOTTOM_CARD_H))
        .p(px(26.))
        .gap(px(30.))
        .child(text(c.t(Tr::StatsDetectedLanguages), 15., FontWeight::BOLD, p.text).line_height(px(24.)));
    let mut rows = Vec::with_capacity(shown);
    for (i, (code, n)) in stats.langs.iter().take(3).enumerate() {
        let pct = *n as f32 / total_langs as f32;
        let col = crate::theme::hex(shades[i]);
        let id = &ids[i];
        let level = if pointed.is_some_and(|pointed| pointed != i) { LANGUAGE_DIMMED } else { 1. };
        // GPUI clips to rectangles: round the outer ends of the first and last segment instead.
        let (first, last) = (i == 0, i + 1 == shown);
        // The fading fill is a child, so the hovered segment itself stays mounted.
        stack =
            stack.child(div().id(id.clone()).h_full().w(relative(pct)).on_hover(ui::hover_tracker(id.clone())).child(
                ui::tweened(ElementId::from((id.clone(), "fill")), level, ms, move |v| {
                    let mut fill = div().size_full().bg(col).opacity(v);
                    if first {
                        fill = fill.rounded_l(px(7.));
                    }
                    if last {
                        fill = fill.rounded_r(px(7.));
                    }
                    fill
                }),
            ));
        let name = SharedString::from(lang_name(code, c.lang));
        let share = SharedString::from(format!("{:.0}%", pct * 100.));
        rows.push(ui::tweened(ElementId::from((id.clone(), "row")), level, ms, move |v| {
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .opacity(v)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(div().size(px(12.)).rounded_full().bg(col))
                        .child(text(name.clone(), 13., FontWeight::MEDIUM, p.text)),
                )
                .child(ui::muted12(&p, share.clone()))
        }));
    }
    card = card.child(stack).children(rows);
    if stats.langs.is_empty() {
        card = card.child(ui::muted12(&p, c.t(Tr::StatsNoData)));
    }
    card
}

fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
