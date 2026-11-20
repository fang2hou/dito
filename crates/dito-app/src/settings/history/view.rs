use std::sync::Arc;

use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::day_list::{HistoryRow, HistorySnapshot};
use super::page::{HistoryPage, RowFocus};
use super::row::expandable;
use super::skeleton::skeleton_column;
use crate::assets::Ic;
use crate::settings::edge_fade::{Edge, edge_fade};
use crate::settings::{Ctx, page_fill};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk};

impl HistoryPage {
    pub(super) fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        if self.placeholder_lang != c.lang {
            self.placeholder_lang = c.lang;
            self.search.update(cx, |s, cx| s.set_placeholder(c.t(Tr::HistorySearch), window, cx));
        }
        // Handle clone (one Arc), not a copy of every row: the render snapshot is swapped wholesale by
        // `loaded`; render never touches the database.
        let snapshot = self.list.data.clone();
        if self.focus_listed.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &snapshot)) {
            self.track_row_focus(&snapshot.rows, window, cx);
            self.focus_listed = Some(snapshot.clone());
        }

        let toolbar = self.toolbar(&c, cx);
        let actions = ui::button(
            "export-history",
            &p,
            &c.t(Tr::WordbookExport),
            BtnKind::Secondary,
            Some(Ic::Download),
            cbk(cx, |this: &mut Self, _, cx| this.export(cx)),
        )
        .into_any_element();
        if self.skeleton {
            return page_fill(
                p,
                "history",
                c.t(Tr::NavHistory),
                Some(actions),
                div().w_full().flex().flex_col().gap(px(24.)).child(toolbar).child(skeleton_column(&p)),
            );
        }
        let list = self.virtual_list(snapshot, &c, cx);
        page_fill(
            p,
            "history",
            c.t(Tr::NavHistory),
            Some(actions),
            div()
                .w_full()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .gap(px(24.))
                .child(toolbar)
                .child(self.faded(list, &p)),
        )
    }

    fn track_row_focus(&mut self, entries: &[HistoryRow], window: &mut Window, cx: &mut Context<Self>) {
        self.row_focus.retain(|id, _| entries.iter().any(|e| e.id == *id && expandable(e)));
        for e in entries.iter().filter(|e| expandable(e)) {
            self.row_focus.entry(e.id).or_insert_with(|| {
                let handle = cx.focus_handle().tab_stop(true);
                let _listeners = ui::focus_notifier(&handle, window, cx);
                RowFocus { handle, _listeners }
            });
        }
    }

    fn virtual_list(&mut self, snapshot: Arc<HistorySnapshot>, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let changed = self.listed.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &snapshot));
        if changed && self.hovered_original.is_some_and(|id| !snapshot.rows.iter().any(|e| e.id == id && expandable(e)))
        {
            self.hovered_original = None;
        }
        if changed {
            self.sync_list_state(snapshot.as_ref());
            self.listed = Some(snapshot.clone());
        }
        let p = c.p;
        let lang = c.lang;
        let build = cx.processor(move |this, index: usize, window, cx| {
            this.day_item(&Ctx { p, lang }, cx, &snapshot, index, window)
        });
        list(self.list_state.clone(), build).flex_1().min_h(px(0.)).into_any_element()
    }

    fn sync_list_state(&mut self, next: &HistorySnapshot) {
        let Some(previous) = self.listed.as_deref() else {
            self.list_state.reset(next.items.len());
            return;
        };
        let old_len = previous.items.len();
        let new_len = next.items.len();
        let prefix = (0..old_len.min(new_len))
            .take_while(|&index| previous.item_identity(index) == next.item_identity(index))
            .count();
        let mut suffix = 0;
        while suffix < old_len - prefix
            && suffix < new_len - prefix
            && previous.item_identity(old_len - suffix - 1) == next.item_identity(new_len - suffix - 1)
        {
            suffix += 1;
        }
        let old_end = old_len - suffix;
        let new_end = new_len - suffix;
        if prefix != old_end || prefix != new_end {
            self.list_state.splice(prefix..old_end, new_end - prefix);
        }
        for index in 0..prefix {
            if !previous.item_eq(index, next, index) {
                self.list_state.remeasure_items(index..index + 1);
            }
        }
        for offset in 0..suffix {
            let old_index = old_end + offset;
            let new_index = new_end + offset;
            if !previous.item_eq(old_index, next, new_index) {
                self.list_state.remeasure_items(new_index..new_index + 1);
            }
        }
    }

    /// Pure background (no id, no listeners), never hit-tested. The positioning context is the list viewport only, so
    /// the fades hug the scrolled content's edges instead of the toolbar's.
    fn faded(&self, list: AnyElement, p: &Palette) -> Div {
        const FADE: f32 = 28.;
        let (scrolled, max_scroll) = scroll_extent(&self.list_state);
        let state = self.list_state.clone();
        let top_a = (scrolled / FADE).clamp(0., 1.);
        let bottom_a = ((max_scroll - scrolled) / FADE).clamp(0., 1.);
        div()
            .relative()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(list)
            // The list settles its extent while it lays out, after the fades were sized from the previous one. When
            // the two differ, one more frame sizes the fades from the settled extent.
            .child(
                canvas(
                    move |_, window, _| {
                        let (now_scrolled, now_max) = scroll_extent(&state);
                        if (now_scrolled - scrolled).abs() > 0.5 || (now_max - max_scroll).abs() > 0.5 {
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_0(),
            )
            .when(top_a > 0.001, |d| d.child(edge_fade(Edge::Top, p.bg, FADE, 0.).opacity(top_a)))
            .when(bottom_a > 0.001, |d| d.child(edge_fade(Edge::Bottom, p.bg, FADE, 0.).opacity(bottom_a)))
    }
}

/// How far the list is scrolled and how far it can scroll.
fn scroll_extent(list: &ListState) -> (f32, f32) {
    (-f32::from(list.scroll_px_offset_for_scrollbar().y), f32::from(list.max_offset_for_scrollbar().y))
}
