use dito_core::i18n::Tr;
use dito_core::store::{HistoryEntry, HistoryKind};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::disclosure::OriginalDisclosure;
use super::page::HistoryPage;
use crate::assets::Ic;
use crate::services;
use crate::settings::Ctx;
use crate::theme::Tint;
use crate::timefmt::hhmm;
use crate::ui::{self, BtnKind, Cb, MenuItem, MenuTone, cbk, text};

type PageAction = Box<dyn Fn(&mut HistoryPage, &mut Context<HistoryPage>)>;

struct RowParts {
    meta: Div,
    body: Div,
    actions: Div,
}

impl HistoryPage {
    pub(super) fn menu_for(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry) -> Div {
        let id = e.id;
        let close: Cb = cbk(cx, |this: &mut Self, _, cx| {
            this.menu = None;
            cx.notify();
        });
        let wrap = |cx: &mut Context<Self>, f: PageAction| -> Cb { cbk(cx, move |this: &mut Self, _, cx| f(this, cx)) };
        let mut items = vec![
            MenuItem {
                icon: Some(Ic::Mic),
                label: c.t(Tr::HistoryTranscribeAgain),
                tone: MenuTone::Default,
                on_click: wrap(cx, Box::new(move |t, cx| t.run_job(id, cx, services::transcribe_again))),
            },
            MenuItem {
                icon: Some(Ic::Sparkles),
                label: c.t(Tr::HistoryPolishAgain),
                tone: MenuTone::Default,
                on_click: wrap(cx, Box::new(move |t, cx| t.run_job(id, cx, services::polish_again))),
            },
        ];
        let raw = e.raw.clone();
        items.push(MenuItem {
            icon: Some(Ic::Copy),
            label: c.t(Tr::HistoryCopyOriginal),
            tone: MenuTone::Default,
            on_click: wrap(
                cx,
                Box::new(move |t, _| {
                    t.copy(&raw);
                }),
            ),
        });
        if let Some(path) = e.audio_path.clone() {
            items.push(MenuItem {
                icon: Some(Ic::Play),
                label: c.t(Tr::HistoryPlayAudio),
                tone: MenuTone::Default,
                on_click: wrap(cx, Box::new(move |t, _| t.st.player.play(std::path::Path::new(&path)))),
            });
        }
        items.push(MenuItem {
            icon: Some(Ic::Trash),
            label: c.t(Tr::ModelsDelete),
            tone: MenuTone::Danger,
            on_click: wrap(cx, Box::new(move |t, cx| t.delete(id, cx))),
        });
        ui::popup_menu(SharedString::from(format!("hist-menu-{id}")), &c.p, items, close)
    }

    pub(super) fn row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        e: &HistoryEntry,
        off: i32,
        last: bool,
        window: &Window,
    ) -> Div {
        let p = &c.p;
        let id = e.id;
        let RowParts { meta, body, actions } = self.kind_parts(c, cx, e, off);
        let actions = actions.child(self.more_control(c, cx, e));

        let body_id = ElementId::from(("hist-body", id as usize));
        let trigger = self.trigger(cx, e, body_id.clone(), meta, body);
        let mut row = div()
            .relative()
            .w_full()
            .px(px(24.))
            .py(px(18.))
            .flex()
            .items_start()
            .gap(px(20.))
            .child(trigger)
            .child(actions);
        if expandable(e) {
            row = row.child(self.original_tip(c, window, id, body_id));
        }
        if !last {
            row = row.border_b_1().border_color(p.border);
        }
        row
    }

    fn kind_parts(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry, off: i32) -> RowParts {
        let p = &c.p;
        let meta = div().flex().items_center().gap(px(10.)).child(
            // Fixed-width slot: times are always 00:00-shaped but the font is proportional, so
            // without this the tag after it starts at a different x on every row.
            div().w(px(38.)).flex_none().child(text(hhmm(e.ts, off), 13., FontWeight::NORMAL, p.muted)),
        );
        let body = div().w_full().flex().flex_col().gap(px(10.));
        let actions = div().flex().items_center().gap(px(8.)).flex_none();
        let parts = RowParts { meta, body, actions };
        match e.kind {
            HistoryKind::Dictation => self.dictation_parts(c, cx, e, parts),
            HistoryKind::Cancelled => self.cancelled_parts(c, cx, e, parts),
            HistoryKind::Failed => self.failed_parts(c, cx, e, parts),
        }
    }

    fn dictation_parts(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry, parts: RowParts) -> RowParts {
        let p = &c.p;
        let id = e.id;
        let RowParts { mut meta, mut body, mut actions } = parts;
        let cfg = self.st.cfg.get();
        meta = meta.child(ui::tag(p, ui::history_style_label(c.lang, &cfg.prompts, &e.style, &e.app), Tint::Blue));
        body = body.child(text(e.final_text().to_string(), 14., FontWeight::NORMAL, p.text).line_height(relative(1.6)));
        if expandable(e) {
            let raw = e.raw.clone();
            let original = c.t(Tr::HistoryOriginal);
            let p = *p;
            let open = self.expanded.contains(&id);
            body = body.gap(px(0.)).child(OriginalDisclosure { id, raw, original, p, open });
        }
        let t = e.final_text().to_string();
        actions = actions.child(ui::copy_button(
            SharedString::from(format!("copy-{id}")),
            p,
            ui::cbk_copy(cx, move |this: &mut Self, _, _| this.copy(&t)),
        ));
        RowParts { meta, body, actions }
    }

    fn cancelled_parts(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry, parts: RowParts) -> RowParts {
        let p = &c.p;
        let RowParts { mut meta, mut body, mut actions } = parts;
        meta = meta.child(ui::tag(p, c.t(Tr::ModelsCancel), Tint::Warning)).child(text(
            format!("{}:{:02}", e.duration_ms / 60_000, e.duration_ms / 1000 % 60),
            12.,
            FontWeight::NORMAL,
            p.muted,
        ));
        body = body.child(text(c.t(Tr::HistoryCancelled), 14., FontWeight::NORMAL, p.muted));
        actions = actions.child(self.retry_button(c, cx, e.id, c.t(Tr::HistoryTranscribeAgain), Ic::Mic));
        RowParts { meta, body, actions }
    }

    fn failed_parts(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry, parts: RowParts) -> RowParts {
        let p = &c.p;
        let RowParts { mut meta, mut body, mut actions } = parts;
        meta = meta.child(ui::tag(p, c.t(Tr::ModelsFailed), Tint::Danger));
        body = body.child(text(
            ui::error_line(c.lang, e.error.as_deref().unwrap_or_default()),
            14.,
            FontWeight::NORMAL,
            p.muted,
        ));
        actions = actions.child(self.retry_button(c, cx, e.id, c.t(Tr::HistoryRetry), Ic::Refresh));
        RowParts { meta, body, actions }
    }

    fn retry_button(&self, c: &Ctx, cx: &mut Context<Self>, id: i64, label: SharedString, icon: Ic) -> AnyElement {
        let p = &c.p;
        if self.busy.contains(&id) {
            return text(c.t(Tr::MenubarWorking), 13., FontWeight::NORMAL, p.muted).into_any_element();
        }
        ui::button(
            SharedString::from(format!("retry-{id}")),
            p,
            &label,
            BtnKind::Secondary,
            Some(icon),
            cbk(cx, move |t: &mut Self, _, cx| t.run_job(id, cx, services::transcribe_again)),
        )
        .into_any_element()
    }

    fn more_control(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry) -> Stateful<Div> {
        let p = &c.p;
        let id = e.id;
        let more_open = self.menu == Some(id);
        let btn = ui::menu_button(
            SharedString::from(format!("more-{id}")),
            p,
            Ic::MoreV,
            more_open,
            cbk(cx, move |this: &mut Self, _, cx| {
                this.menu = if this.menu == Some(id) { None } else { Some(id) };
                cx.notify();
            }),
        );
        // Same guard as the model rows: eat clicks over the ⋮ so dismissing the menu with a
        // second press never toggles the row underneath.
        let mut w = div()
            .relative()
            .id(ElementId::from(SharedString::from(format!("more-wrap-{id}"))))
            .child(btn)
            .on_click(ui::click_cb(cbk(cx, |_: &mut Self, _, cx| cx.stop_propagation())));
        if more_open {
            w = w.child(self.menu_for(c, cx, e));
        }
        w
    }

    fn trigger(
        &self,
        cx: &mut Context<Self>,
        e: &HistoryEntry,
        body_id: ElementId,
        meta: Div,
        body: Div,
    ) -> Stateful<Div> {
        let id = e.id;
        let mut trigger =
            div().id(body_id.clone()).flex_1().min_w(px(0.)).flex().flex_col().gap(px(10.)).child(meta).child(body);
        if expandable(e) {
            trigger = trigger
                .cursor_pointer()
                .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                    if update_original_hover(&mut this.hovered_original, id, *over) {
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_original(id, cx)));
            if let Some(focus) = self.row_focus.get(&id) {
                trigger = trigger.track_focus(focus).on_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
                    if matches!(ev.keystroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        this.toggle_original(id, cx);
                    }
                }));
            }
        }
        trigger
    }

    fn original_tip(&self, c: &Ctx, window: &Window, id: i64, body_id: ElementId) -> Deferred {
        let p = &c.p;
        let focused = window.last_input_was_keyboard() && self.row_focus.get(&id).is_some_and(|f| f.is_focused(window));
        deferred(
            // Anchor to the entire row, including its action column. The
            // bubble's centre sits on the inside 1px bottom border.
            div()
                .absolute()
                .top_full()
                .mt(px(-ui::TIP_BUBBLE_H / 2. - 0.5))
                .left(px(0.5))
                .right(px(0.5))
                .flex()
                .justify_center()
                .child(ui::history_tip(
                    p,
                    body_id.clone(),
                    c.t(Tr::HistoryShowOriginal),
                    c.t(Tr::HistoryHideOriginal),
                    self.expanded.contains(&id),
                    self.hovered_original == Some(id) || focused,
                )),
        )
        .with_priority(2)
    }
}

/// A late leave from the previous row must not clear the new row's hint.
pub(super) fn update_original_hover(current: &mut Option<i64>, id: i64, over: bool) -> bool {
    let next = if over { Some(id) } else { current.filter(|owner| *owner != id) };
    let changed = *current != next;
    *current = next;
    changed
}

pub(super) fn expandable(entry: &HistoryEntry) -> bool {
    entry.kind == HistoryKind::Dictation && entry.polished.is_some() && !entry.raw.is_empty()
}

#[cfg(test)]
mod tests {
    use super::update_original_hover;

    #[test]
    fn rapid_row_transfers_keep_only_the_current_hint() {
        let mut owner = None;
        update_original_hover(&mut owner, 1, true);
        update_original_hover(&mut owner, 2, true);
        assert_eq!(owner, Some(2));
        update_original_hover(&mut owner, 1, false);
        assert_eq!(owner, Some(2), "a late leave belongs to the old row");
        update_original_hover(&mut owner, 2, false);
        assert_eq!(owner, None, "leaving the active row dismisses the hint");
        update_original_hover(&mut owner, 1, false);
        assert_eq!(owner, None, "repeated leaves must not restore an old hint");
    }
}
