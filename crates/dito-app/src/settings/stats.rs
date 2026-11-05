use std::sync::Arc;

use dito_core::i18n::Tr;
use gpui_kit::*;

use super::query_state::{Finish, QueryState, Snapshot, apply_snapshot};
use super::{Ctx, watch_config};
use crate::services;
use crate::state::AppState;
use crate::timefmt::{day_start, local_day, now, weekday, ymd};
use crate::ui::{self, cbk_idx, text};
use dito_core::store::Stats;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    Week,
    Month,
    All,
}

/// Weekday initials, Monday-first to match `crate::timefmt::weekday`.
const WEEKDAYS: [Tr; 7] = [
    Tr::StatsWeekdayMon,
    Tr::StatsWeekdayTue,
    Tr::StatsWeekdayWed,
    Tr::StatsWeekdayThu,
    Tr::StatsWeekdayFri,
    Tr::StatsWeekdaySat,
    Tr::StatsWeekdaySun,
];

pub struct StatsPage {
    st: Arc<AppState>,
    range: Range,
    /// Owned stats snapshot, swapped by `loaded`; render never touches the database.
    stats: Stats,
    /// Last query failure, kept for logging/debugging — never rendered as UI copy.
    error: Option<anyhow::Error>,
    /// Load lifecycle: active gate, single-flight queries, generation checks.
    queries: QueryState,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

impl StatsPage {
    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);
        Self {
            motion: ui::MotionScope::new(),
            st,
            range: Range::Week,
            stats: Stats::default(),
            error: None,
            queries: QueryState::new(),
        }
    }

    /// First second included in the current range (None = all time).
    fn since(&self) -> Option<i64> {
        let off = dito_platform::local_utc_offset_secs();
        let today = local_day(now(), off);
        match self.range {
            Range::Week => Some(day_start(today - 6, off)),
            Range::Month => Some(day_start(today - 29, off)),
            Range::All => None,
        }
    }

    /// Same-input reload (page entry). Collapses into the running query; its result still
    /// applies when it lands.
    fn load(&mut self, cx: &mut Context<Self>) {
        if let Some(ticket) = self.queries.request() {
            self.launch(ticket, cx);
        }
    }

    /// The range changed: any result still running under the old range is stale and will be
    /// dropped when it lands.
    fn changed(&mut self, cx: &mut Context<Self>) {
        if let Some(ticket) = self.queries.restart() {
            self.launch(ticket, cx);
        }
    }

    fn launch(&mut self, ticket: u64, cx: &mut Context<Self>) {
        let st = self.st.clone();
        let since = self.since();
        cx.spawn(async move |this, cx| {
            let r = services::history_stats(st, since).await;
            let _ = this.update(cx, |t, cx| t.loaded(ticket, r, cx));
        })
        .detach();
    }

    fn loaded(&mut self, ticket: u64, result: anyhow::Result<Stats>, cx: &mut Context<Self>) {
        match self.queries.finish(ticket) {
            Finish::Apply => self.apply(result, cx),
            Finish::ApplyAndReload(next) => {
                self.apply(result, cx);
                self.launch(next, cx);
            }
            Finish::Superseded(next) => self.launch(next, cx),
            Finish::Ignored => {}
        }
    }

    fn apply(&mut self, result: anyhow::Result<Stats>, cx: &mut Context<Self>) {
        match apply_snapshot(&mut self.stats, result) {
            Snapshot::Replaced => cx.notify(),
            Snapshot::Unchanged => {}
            Snapshot::Failed(e) => {
                tracing::warn!("stats query failed: {e:#}");
                self.error = Some(e);
                return;
            }
        }
        self.error = None;
    }

    /// Page visibility lifecycle, driven by `Settings`. Stats loads on entry and on range
    /// changes; while inactive no queries run and late results are ignored.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.queries.set_active(active);
        if active {
            self.load(cx);
        }
    }
}

fn lang_name(code: &str) -> String {
    match code {
        "ja" => "日本語".into(),
        "en" => "English".into(),
        "zh" => "中文".into(),
        "" => "—".into(),
        other => other.to_string(),
    }
}

impl StatsPage {
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    /// The page's scope, for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for StatsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(_window, cx))
    }
}

impl StatsPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let off = dito_platform::local_utc_offset_secs();
        let today = local_day(now(), off);
        let days_back: i64 = match self.range {
            Range::Week => 7,
            Range::Month | Range::All => 30,
        };
        let stats = &self.stats;

        let range_seg = ui::seg(
            "stats-range",
            &p,
            &[c.t(Tr::HistoryThisWeek), c.t(Tr::HistoryThisMonth), c.t(Tr::HistoryAll)],
            match self.range {
                Range::Week => 0,
                Range::Month => 1,
                Range::All => 2,
            },
            cbk_idx(cx, |this: &mut Self, i, _, cx| {
                this.range = [Range::Week, Range::Month, Range::All][i];
                this.changed(cx);
                cx.notify();
            }),
        );

        // headline numbers
        let saved = if stats.saved_minutes >= 60.0 {
            format!("{:.1} {}", stats.saved_minutes / 60.0, c.t(Tr::UnitsHours))
        } else {
            format!("{:.0} {}", stats.saved_minutes, c.t(Tr::UnitsMinutes))
        };
        let cells = [
            (c.t(Tr::StatsCharacters), format!("{} {}", fmt_int(stats.chars), c.t(Tr::UnitsChars))),
            (c.t(Tr::StatsAverageSpeed), format!("{:.0} {}", stats.avg_cpm, c.t(Tr::UnitsCharsPerMin))),
            (c.t(Tr::StatsTimeSaved), saved),
            (c.t(Tr::StatsRecordings), format!("{} {}", stats.recordings, c.t(Tr::UnitsTimes))),
        ];
        let n = cells.len();
        // Content width = window - sidebar (232) - page padding (2 x 56); 4 KPI columns when
        // there is room, else a 2 x 2 grid so CJK units never push cells past the card edge.
        let avail = f32::from(window.viewport_size().width) - 232. - 112.;
        // Keep each number and its localized unit together; use two columns if the actual
        // values need more room, rather than breaking a rate across two lines.
        let min_column =
            cells.iter().map(|(_, value)| ui::text_width(value, 26., FontWeight::MEDIUM) + 52.).fold(180., f32::max);
        let (per_row, per_row_u16) = if avail >= min_column * 4. { (4usize, 4u16) } else { (2, 2) };
        let rows = n.div_ceil(per_row);
        let mut head = ui::card(&p).grid().grid_cols(per_row_u16);
        for (i, (k, v)) in cells.into_iter().enumerate() {
            let mut cell = div()
                .px(px(26.))
                .py(px(18.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(text(k, 13., FontWeight::NORMAL, p.muted))
                .child(text(v, 26., FontWeight::MEDIUM, p.text).whitespace_nowrap());
            if (i + 1) % per_row != 0 {
                cell = cell.border_r_1().border_color(p.border);
            }
            if per_row < n && i < per_row * (rows - 1) {
                cell = cell.border_b_1().border_color(p.border);
            }
            head = head.child(cell);
        }

        // per-day bars
        let by_day: std::collections::HashMap<String, u64> = stats.per_day.iter().cloned().collect();
        let series: Vec<(i64, u64)> = (0..days_back)
            .map(|i| today - (days_back - 1 - i))
            .map(|d| (d, by_day.get(&ymd(d)).copied().unwrap_or(0)))
            .collect();
        let max = series.iter().map(|(_, v)| *v).max().unwrap_or(0).max(1);
        let peak = series.iter().enumerate().max_by_key(|(_, (_, v))| *v).map(|(i, _)| i);
        let bar_w = if days_back == 7 { 52. } else { 14. };
        let mut bars = div().w_full().h(px(170.)).flex().items_end().justify_between().px(px(10.));
        let mut labels = div().w_full().flex().justify_between().px(px(10.));
        for (i, (d, v)) in series.iter().enumerate() {
            let is_peak = Some(i) == peak && *v > 0;
            let h = (*v as f32 / max as f32 * 120.).max(if *v > 0 { 6. } else { 0. });
            bars = bars.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(if days_back == 7 {
                        text(
                            fmt_int(*v),
                            11.,
                            if is_peak { FontWeight::MEDIUM } else { FontWeight::NORMAL },
                            if is_peak { p.text } else { p.muted },
                        )
                    } else {
                        div()
                    })
                    .child(div().w(px(bar_w)).h(px(h)).rounded(px(if days_back == 7 { 12. } else { 5. })).bg(
                        if is_peak {
                            p.accent
                        } else if p.dark {
                            crate::theme::hex("#C2A765")
                        } else {
                            crate::theme::hex("#F6DA97")
                        },
                    )),
            );
            let label = if days_back == 7 {
                c.lang.tr(WEEKDAYS[weekday(*d)]).to_string()
            } else if i % 5 == 0 || i + 1 == series.len() {
                ymd(*d)[5..].to_string()
            } else {
                String::new()
            };
            labels = labels.child(div().w(px(bar_w)).flex().justify_center().child(ui::muted12(&p, label)));
        }
        let chart = ui::card(&p)
            .px(px(26.))
            .py(px(22.))
            .gap(px(16.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text(c.t(Tr::StatsCharactersPerDay), 15., FontWeight::BOLD, p.text))
                    .child(ui::muted12(&p, format!("{} {}", c.t(Tr::StatsMax), fmt_int(max)))),
            )
            .child(bars)
            .child(labels);

        // apps
        let total_apps: u64 = stats.apps.iter().map(|(_, n)| *n).sum::<u64>().max(1);
        let mut apps = ui::card(&p).flex_1().p(px(26.)).gap(px(18.)).child(text(
            c.t(Tr::StatsTopApps),
            15.,
            FontWeight::BOLD,
            p.text,
        ));
        for (i, (name, n)) in stats.apps.iter().take(5).enumerate() {
            let pct = *n as f32 / total_apps as f32;
            apps = apps.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .w_full()
                            .flex()
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
        if stats.apps.is_empty() {
            apps = apps.child(ui::muted12(&p, c.t(Tr::StatsNoData)));
        }

        // languages
        let total_langs: u64 = stats.langs.iter().map(|(_, n)| *n).sum::<u64>().max(1);
        let shades = if p.dark { ["#E0C47D", "#C2A765", "#826F41"] } else { ["#685218", "#C2A765", "#F6DA97"] };
        let shown = stats.langs.len().min(3);
        let mut stack = div().w_full().h(px(14.)).flex();
        let mut legend = div().w_full().flex().flex_col().gap(px(14.));
        for (i, (code, n)) in stats.langs.iter().take(3).enumerate() {
            let pct = *n as f32 / total_langs as f32;
            let col = crate::theme::hex(shades[i]);
            // GPUI clips to rectangles: round the outer ends of the first and last segment instead.
            let mut seg = div().h_full().w(relative(pct)).bg(col);
            if i == 0 {
                seg = seg.rounded_l(px(7.));
            }
            if i + 1 == shown {
                seg = seg.rounded_r(px(7.));
            }
            stack = stack.child(seg);
            legend = legend.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(div().size(px(12.)).rounded_full().bg(col))
                            .child(text(lang_name(code), 13., FontWeight::MEDIUM, p.text)),
                    )
                    .child(ui::muted12(&p, format!("{:.0}%", pct * 100.))),
            );
        }
        let langs = ui::card(&p)
            .flex_1()
            .p(px(26.))
            .gap(px(18.))
            .child(text(c.t(Tr::StatsRecognizedLanguages), 15., FontWeight::BOLD, p.text))
            .child(stack)
            .child(legend);

        super::page(
            p,
            "stats",
            c.t(Tr::NavStats),
            Some(range_seg.into_any_element()),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(head)
                .child(chart)
                .child(div().w_full().flex().items_start().gap(px(20.)).child(apps).child(langs)),
        )
    }
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

#[cfg(test)]
mod tests {
    use super::fmt_int;

    #[test]
    fn thousands_separators() {
        assert_eq!(fmt_int(0), "0");
        assert_eq!(fmt_int(999), "999");
        assert_eq!(fmt_int(8420), "8,420");
        assert_eq!(fmt_int(1_234_567), "1,234,567");
    }
}
