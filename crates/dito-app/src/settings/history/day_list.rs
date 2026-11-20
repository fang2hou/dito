use dito_core::i18n::{Lang, Tr};
use dito_core::intent::Intent;
use dito_core::prompts::Prompts;
use dito_core::store::{CommandOutcome, HistoryEntry, HistoryKind};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::HistoryPage;
use crate::settings::Ctx;
use crate::timefmt::{hhmm, local_day, now, ymd};
use crate::ui::{self, text};

#[derive(Clone, PartialEq)]
pub(super) struct HistoryRow {
    pub(super) id: i64,
    pub(super) ts: i64,
    pub(super) kind: HistoryKind,
    pub(super) duration: SharedString,
    pub(super) raw: SharedString,
    pub(super) result: Option<SharedString>,
    pub(super) final_text: SharedString,
    pub(super) error: Option<SharedString>,
    pub(super) audio_path: Option<SharedString>,
    pub(super) source: Option<SharedString>,
    pub(super) target: SharedString,
    pub(super) time: SharedString,
    pub(super) command_tool: Option<Intent>,
    pub(super) command_outcome: Option<CommandOutcome>,
}

#[derive(Clone, PartialEq)]
pub(super) enum HistoryItem {
    Empty,
    Heading { day: i64, label: SharedString, first: bool },
    Row { index: usize, first: bool, last: bool },
}

#[derive(PartialEq)]
pub struct HistorySnapshot {
    pub(super) rows: Vec<HistoryRow>,
    pub(super) items: Vec<HistoryItem>,
}

impl HistorySnapshot {
    pub(super) fn new(entries: Vec<HistoryEntry>, lang: Lang, prompts: &Prompts, off: i32) -> Self {
        let today = local_day(now(), off);
        let rows: Vec<HistoryRow> = entries
            .into_iter()
            .map(|entry| {
                let error = entry.error.as_deref().map(|error| ui::error_line(lang, error).into());
                let target = ui::history_target_label(lang, prompts, &entry.style, &entry.app);
                let duration =
                    SharedString::from(format!("{}:{:02}", entry.duration_ms / 60_000, entry.duration_ms / 1000 % 60));
                let time = SharedString::from(hhmm(entry.ts, off));
                let command_tool = entry.command.as_ref().and_then(|details| details.tool);
                let command_outcome = entry.command.as_ref().map(|details| details.outcome);
                let raw = SharedString::from(entry.raw);
                let result = entry.polished.map(SharedString::from);
                let final_text = result.clone().unwrap_or_else(|| raw.clone());
                HistoryRow {
                    id: entry.id,
                    ts: entry.ts,
                    kind: entry.kind,
                    duration,
                    raw,
                    result,
                    final_text,
                    error,
                    audio_path: entry.audio_path.map(SharedString::from),
                    source: entry.source.map(SharedString::from),
                    target,
                    time,
                    command_tool,
                    command_outcome,
                }
            })
            .collect();
        let mut items = Vec::new();
        if rows.is_empty() {
            items.push(HistoryItem::Empty);
            return Self { rows, items };
        }
        let mut start = 0;
        let mut first_group = true;
        while start < rows.len() {
            let day = local_day(rows[start].ts, off);
            let end = rows[start..]
                .iter()
                .position(|row| local_day(row.ts, off) != day)
                .map_or(rows.len(), |offset| start + offset);
            let label = if day == today {
                lang.tr(Tr::HistoryToday).into()
            } else if day == today - 1 {
                lang.tr(Tr::HistoryYesterday).into()
            } else {
                ymd(day).into()
            };
            items.push(HistoryItem::Heading { day, label, first: first_group });
            first_group = false;
            for index in start..end {
                items.push(HistoryItem::Row { index, first: index == start, last: index + 1 == end });
            }
            start = end;
        }
        Self { rows, items }
    }

    pub(super) fn item_index(&self, id: i64) -> Option<usize> {
        self.items.iter().position(|item| matches!(item, HistoryItem::Row { index, .. } if self.rows[*index].id == id))
    }

    pub(super) fn item_identity(&self, index: usize) -> (u8, i64) {
        match self.items[index] {
            HistoryItem::Empty => (0, 0),
            HistoryItem::Heading { day, .. } => (1, day),
            HistoryItem::Row { index, .. } => (2, self.rows[index].id),
        }
    }

    pub(super) fn item_eq(&self, index: usize, other: &Self, other_index: usize) -> bool {
        match (&self.items[index], &other.items[other_index]) {
            (HistoryItem::Empty, HistoryItem::Empty) => true,
            (HistoryItem::Heading { label: a, first: af, .. }, HistoryItem::Heading { label: b, first: bf, .. }) => {
                a == b && af == bf
            }
            (
                HistoryItem::Row { index: a, first: af, last: al },
                HistoryItem::Row { index: b, first: bf, last: bl },
            ) => af == bf && al == bl && self.rows[*a] == other.rows[*b],
            _ => false,
        }
    }
}

impl HistoryPage {
    pub(super) fn day_item(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        snapshot: &HistorySnapshot,
        index: usize,
        window: &Window,
    ) -> AnyElement {
        match &snapshot.items[index] {
            HistoryItem::Empty => div()
                .w_full()
                .py(px(40.))
                .flex()
                .justify_center()
                .child(ui::muted12(&c.p, c.t(Tr::HistoryNoHistoryYet)).text_size(px(14.)))
                .into_any_element(),
            HistoryItem::Heading { label, first, .. } => div()
                .w_full()
                .pt(px(if *first { 0. } else { 20. }))
                .pb(px(12.))
                .child(text(label.clone(), 16., FontWeight::BOLD, c.p.text))
                .into_any_element(),
            HistoryItem::Row { index, first, last } => {
                let row = self.row(c, cx, &snapshot.rows[*index], *last, window);
                ui::card_segment(&c.p, *first, *last).child(row).into_any_element()
            }
        }
    }
}
