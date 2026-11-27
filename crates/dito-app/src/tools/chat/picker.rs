//! The meeting picker above the Add meeting action: a search field, meetings grouped by day, and the count with the primary add
//! (`design:tools/chat`, 23.05.5).

use dito_core::i18n::{Tr, fmt};
use dito_core::store::TranscriptSummary;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::ChatPage;
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::timefmt::{hhmm, local_day, now, ymd};
use crate::ui::{self, BtnKind, InputEvent, InputState};

const WIDTH: f32 = 360.;
const LIST_MAX_H: f32 = 300.;
const ROW_H: f32 = 40.;
const MARKER: f32 = 18.;

pub(super) struct Picker {
    search: Entity<InputState>,
    query: String,
    meetings: Vec<TranscriptSummary>,
    /// Transcript ids, in the order they were ticked.
    pub selected: Vec<i64>,
}

impl Picker {
    pub fn new(
        meetings: Vec<TranscriptSummary>,
        attached: Vec<i64>,
        lang: dito_core::i18n::Lang,
        window: &mut Window,
        cx: &mut Context<ChatPage>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::ChatSearchMeetings)));
        cx.subscribe_in(&search, window, |page: &mut ChatPage, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change)
                && let Some(picker) = &mut page.picker
            {
                picker.query = state.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        search.update(cx, |state, cx| state.focus(window, cx));
        Self { search, query: String::new(), meetings, selected: attached }
    }

    fn toggle(&mut self, id: i64) {
        match self.selected.iter().position(|s| *s == id) {
            Some(at) => {
                self.selected.remove(at);
            }
            None => self.selected.push(id),
        }
    }

    fn shown(&self) -> Vec<&TranscriptSummary> {
        let needle = self.query.trim().to_lowercase();
        self.meetings
            .iter()
            .filter(|m| {
                needle.is_empty()
                    || m.title.to_lowercase().contains(&needle)
                    || m.preview.to_lowercase().contains(&needle)
            })
            .collect()
    }
}

pub(super) fn view(picker: &Picker, c: &Ctx, cx: &mut Context<ChatPage>) -> impl IntoElement {
    let p = c.p;
    let close = ui::cbk(cx, |page, _, cx| {
        page.picker = None;
        cx.notify();
    });
    let off = dito_platform::local_utc_offset_secs();
    let today = local_day(now(), off);
    let shown = picker.shown();
    let mut list = div().id("chat-picker-list").w_full().max_h(px(LIST_MAX_H)).overflow_y_scroll().flex().flex_col();
    if shown.is_empty() {
        list = list.child(div().h(px(ROW_H)).px(px(12.)).flex().items_center().child(ui::text(
            c.t(Tr::ChatNoMeetings),
            13.,
            FontWeight::NORMAL,
            p.muted,
        )));
    }
    let mut last_day = None;
    for meeting in shown {
        let day = if meeting.ended_ts.is_none() { today } else { local_day(meeting.started_ts, off) };
        if last_day != Some(day) {
            last_day = Some(day);
            let heading = if day == today {
                c.t(Tr::HistoryToday).to_string()
            } else if day == today - 1 {
                c.t(Tr::HistoryYesterday).to_string()
            } else {
                ymd(day).replace('-', "/")
            };
            list = list.child(div().h(px(28.)).px(px(12.)).pt(px(8.)).flex().items_center().child(ui::text(
                heading,
                11.,
                FontWeight::MEDIUM,
                p.muted,
            )));
        }
        list = list.child(row(picker, meeting, off, c, cx));
    }
    let count = fmt::chat_selected_count(c.lang, &picker.selected.len().to_string());
    let footer = div()
        .h(px(52.))
        .flex_none()
        .mt(px(4.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_between()
        .border_t_1()
        .border_color(p.border)
        .child(ui::text(count, 12., FontWeight::NORMAL, p.muted).line_height(px(18.)))
        .child(ui::button(
            "chat-picker-add",
            &p,
            &c.t(Tr::WordbookAdd),
            BtnKind::Primary,
            None,
            ui::cbk(cx, |page, _, cx| page.confirm_picker(cx)),
        ));
    ui::menu_panel(ElementId::from("chat-picker"), &p, close)
        .w(px(WIDTH))
        .gap(px(0.))
        .child(ui::text_input(&p, &picker.search, Some(Ic::Search)))
        .child(list)
        .child(footer)
}

fn row(picker: &Picker, meeting: &TranscriptSummary, off: i32, c: &Ctx, cx: &mut Context<ChatPage>) -> Stateful<Div> {
    let p = c.p;
    let id = meeting.id;
    let live = meeting.ended_ts.is_none();
    let selected = picker.selected.contains(&id);
    let key = ElementId::from(("chat-picker-row", id as usize));
    let title = if meeting.title.trim().is_empty() {
        format!("{} {}", ymd(local_day(meeting.started_ts, off)).replace('-', "/"), hhmm(meeting.started_ts, off))
    } else {
        meeting.title.trim().to_owned()
    };
    let mut trailing = div().flex_none().flex().items_center().gap(px(10.));
    if live {
        trailing = trailing.child(ui::text(c.t(Tr::TranscriptionRecording), 12., FontWeight::NORMAL, p.danger_text));
    } else {
        trailing = trailing.child(ui::text(hhmm(meeting.started_ts, off), 12., FontWeight::NORMAL, p.muted));
        if let Some(length) = meeting.length_ms {
            trailing = trailing.child(badge(&p, crate::timefmt::clock(length)));
        }
    }
    let mut row = div()
        .id(key.clone())
        .relative()
        .h(px(ROW_H))
        .w_full()
        .flex_none()
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded(px(8.))
        .cursor_pointer()
        .on_hover(ui::hover_tracker(key.clone()))
        .child(ui::hover_fill(&key, Some(8.), p.seg))
        .child(marker(selected, &p))
        .child(
            ui::text(title, 13., if selected { FontWeight::MEDIUM } else { FontWeight::NORMAL }, p.text)
                .line_height(px(19.))
                .flex_1()
                .min_w(px(0.))
                .truncate(),
        )
        .child(trailing);
    row = row.on_click(ui::click_cb(ui::cbk(cx, move |page, _, cx| {
        if let Some(picker) = &mut page.picker {
            picker.toggle(id);
            cx.notify();
        }
    })));
    row
}

fn marker(selected: bool, p: &Palette) -> Div {
    let ring = div().size(px(MARKER)).flex_none().flex().items_center().justify_center().rounded_full();
    if selected {
        ring.bg(p.accent).child(ui::icon(Ic::Check, 12., ui::white()))
    } else {
        ring.border_1().border_color(p.border_strong)
    }
}

fn badge(p: &Palette, label: String) -> Div {
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
        .child(ui::text(label, 11., FontWeight::MEDIUM, p.model_badge_text).line_height(px(13.)))
}
