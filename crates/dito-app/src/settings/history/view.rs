use std::cell::RefCell;

use dito_core::i18n::Tr;
use dito_core::store::HistoryEntry;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::HistoryPage;
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
        // Handle clone (one Arc), not a copy of every row: the snapshot is swapped wholesale by
        // `loaded`; render never touches the database.
        let snapshot = self.list.data.clone();
        let entries: &[HistoryEntry] = if self.skeleton { &[] } else { &snapshot };
        self.track_row_focus(&snapshot, entries, window, cx);

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
        let items = self.day_items(&c, cx, entries, window);
        let list = self.virtual_list(items, entries);
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

    /// One focus handle per row that offers the original text, so keyboard users can open it.
    fn track_row_focus(
        &mut self,
        snapshot: &[HistoryEntry],
        entries: &[HistoryEntry],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.row_focus.retain(|id, _| snapshot.iter().any(|e| e.id == *id && expandable(e)));
        for e in entries.iter().filter(|e| expandable(e)) {
            self.row_focus.entry(e.id).or_insert_with(|| {
                let focus = cx.focus_handle().tab_stop(true);
                cx.on_focus_in(&focus, window, |_, _, cx| cx.notify()).detach();
                cx.on_focus_out(&focus, window, |_, _, _, cx| cx.notify()).detach();
                focus
            });
        }
    }

    /// The virtualized list over `items`. The slot hands each element out exactly once per frame.
    fn virtual_list(&mut self, items: Vec<Option<AnyElement>>, entries: &[HistoryEntry]) -> AnyElement {
        if self.hovered_original.is_some_and(|id| !entries.iter().any(|e| e.id == id && expandable(e))) {
            self.hovered_original = None;
        }
        let n = items.len();
        if n != self.listed {
            self.listed = n;
            self.list_state.reset(n);
        }
        let items = RefCell::new(items);
        list(self.list_state.clone(), move |ix, _, _| {
            items.borrow_mut().get_mut(ix).and_then(|slot| slot.take()).unwrap_or_else(|| {
                div().h(px(80.)).into_any_element() // fallback: state counted an item we dropped
            })
        })
        .flex_1()
        .min_h(px(0.))
        .into_any_element()
    }

    /// Edge fades: where content continues above/below, dissolve it into the page so the cut
    /// reads as "scrollable". Pure background (no id, no listeners) — never hit-tested.
    /// The positioning context is the list viewport only, so the fades hug the scrolled content's
    /// edges instead of the toolbar's.
    fn faded(&self, list: AnyElement, p: &Palette) -> Div {
        const FADE: f32 = 28.;
        let scrolled = -f32::from(self.list_state.scroll_px_offset_for_scrollbar().y);
        let max_scroll = f32::from(self.list_state.max_offset_for_scrollbar().y);
        let top_a = (scrolled / FADE).clamp(0., 1.);
        let bottom_a = ((max_scroll - scrolled) / FADE).clamp(0., 1.);
        div()
            .relative()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(list)
            .when(top_a > 0.001, |d| d.child(edge_fade(Edge::Top, p.bg, FADE, 0.).opacity(top_a)))
            .when(bottom_a > 0.001, |d| d.child(edge_fade(Edge::Bottom, p.bg, FADE, 0.).opacity(bottom_a)))
    }
}
