use dito_core::i18n::Tr;
use dito_core::store::HistoryEntry;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::HistoryPage;
use crate::settings::Ctx;
use crate::timefmt::{local_day, now, ymd};
use crate::ui::{self, text};

impl HistoryPage {
    /// Only the visible slice is laid out and painted, so scrolling stays smooth however long the log grows. Element
    /// building here is cheap struct construction; text shaping happens per visible item.
    pub(super) fn day_items(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        entries: &[HistoryEntry],
        window: &Window,
    ) -> Vec<Option<AnyElement>> {
        let p = &c.p;
        let off = dito_platform::local_utc_offset_secs();
        let today = local_day(now(), off);
        let mut items: Vec<Option<AnyElement>> = Vec::new();
        if entries.is_empty() {
            items.push(Some(
                div()
                    .w_full()
                    .py(px(40.))
                    .flex()
                    .justify_center()
                    .child(ui::muted12(p, c.t(Tr::HistoryNoHistoryYet)).text_size(px(14.)))
                    .into_any_element(),
            ));
        }
        let mut i = 0;
        let mut first_group = true;
        while i < entries.len() {
            let day = local_day(entries[i].ts, off);
            let end =
                entries[i..].iter().position(|e| local_day(e.ts, off) != day).map(|n| i + n).unwrap_or(entries.len());
            let heading = if day == today {
                c.t(Tr::HistoryToday).to_string()
            } else if day == today - 1 {
                c.t(Tr::HistoryYesterday).to_string()
            } else {
                ymd(day)
            };
            let pad_top = if first_group { 0. } else { 20. };
            first_group = false;
            items.push(Some(
                div()
                    .w_full()
                    .pt(px(pad_top))
                    .pb(px(12.))
                    .child(text(heading, 16., FontWeight::BOLD, p.text))
                    .into_any_element(),
            ));
            let mut card = ui::card(p);
            for (k, e) in entries[i..end].iter().enumerate() {
                card = card.child(self.row(c, cx, e, off, i + k + 1 == end, window));
            }
            items.push(Some(card.into_any_element()));
            i = end;
        }
        items
    }
}
