//! `design:settings/status`, Log: the app's own log lines since launch as plain monospaced text.

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::StatusPage;
use crate::logbook::LogLine;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, text};

const TOOLBAR_H: f32 = 54.;
const VIEW_H: f32 = 300.;
const LINE_H: f32 = 20.;
const FONT: &str = "Menlo";
/// A view scrolled up by more than this many points is left alone; within it, it counts as at the bottom.
const AT_BOTTOM_SLACK: f32 = 1.;

/// Everything [`spans`] reads from the palette.
#[derive(Clone, Copy, PartialEq)]
struct SpanColors {
    head: Hsla,
    severe: Hsla,
}

/// The log as the view draws it, kept between renders: joining a full log is too much work for every frame.
pub(super) struct LogText {
    revision: u64,
    colors: SpanColors,
    text: SharedString,
    spans: Rc<[(Range<usize>, Hsla)]>,
}

impl StatusPage {
    pub(super) fn refresh_log_text(&mut self, p: &Palette) {
        // Read before the lines: a line is stored before its tick, so a line that lands in between is drawn now and
        // rebuilt once more, never left out.
        let revision = self.st.log.revision();
        let colors = SpanColors { head: p.muted, severe: p.danger_text };
        if self.log_text.as_ref().is_some_and(|log| log.revision == revision && log.colors == colors) {
            return;
        }
        let lines = self.st.log.lines();
        self.log_text =
            Some(LogText { revision, colors, text: joined(&lines).into(), spans: spans(&lines, colors).into() });
    }

    pub(super) fn log_group(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let card = ui::card(&p)
            .child(self.log_toolbar(c, cx))
            .child(div().w_full().h(px(1.)).bg(p.border))
            .child(self.log_view(c));
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(text(c.t(Tr::StatusLogTitle), 13., FontWeight::BOLD, p.muted))
            .child(card)
    }

    fn log_toolbar(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let weak = cx.entity().downgrade();
        let toggle = Rc::new(move |on, _: &mut Window, app: &mut App| {
            weak.update(app, |page, cx| {
                page.st.log.set_detail(on);
                cx.notify();
            })
            .ok();
        });
        let key = ElementId::from("log-copy");
        ui::tip_label(key.clone(), c.t(Tr::HudCopy));
        let copy = ui::copy_button(
            key.clone(),
            &p,
            ui::cbk_copy(cx, |this: &mut Self, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(joined(&this.st.log.lines())));
                true
            }),
        )
        .child(ui::tip_above(&p, &key));
        div()
            .w_full()
            .h(px(TOOLBAR_H))
            .px(px(24.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(ui::switch("log-detail", &p, self.st.log.detail(), toggle))
                    .child(text(c.t(Tr::StatusLogDetail), 13., FontWeight::MEDIUM, p.text)),
            )
            .child(copy)
    }

    /// Newest line at the bottom. It follows new lines while it is at the bottom and stays where the user left it
    /// after scrolling up.
    fn log_view(&self, c: &Ctx) -> Stateful<Div> {
        let p = c.p;
        let scroll = &self.log_scroll;
        // The offset runs from zero at the top to minus the maximum at the bottom.
        if scroll.max_offset().y + scroll.offset().y <= px(AT_BOTTOM_SLACK) {
            scroll.scroll_to_bottom();
        }
        let view = div()
            .id("log-view")
            .track_scroll(scroll)
            // GPUI hands a wheel event to every scroll container under the pointer, so without this the page scrolls
            // along with the log, and takes over once the log reaches its first or last line.
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .w_full()
            .h(px(VIEW_H))
            .px(px(24.))
            .py(px(10.))
            .overflow_y_scroll()
            .font_family(FONT)
            .text_size(px(11.))
            .line_height(px(LINE_H))
            .text_color(p.text);
        let Some(log) = self.log_text.as_ref().filter(|log| !log.text.is_empty()) else {
            return view;
        };
        view.child(ui::selectable_spans(&p, "log-text", log.text.clone(), log.spans.iter().cloned()))
    }
}

fn joined(lines: &[Arc<LogLine>]) -> String {
    lines.iter().map(|line| line.text()).collect::<Vec<_>>().join("\n")
}

/// The head columns muted, and the level in the danger color for a warning or an error; the message keeps the base
/// color.
fn spans(lines: &[Arc<LogLine>], colors: SpanColors) -> Vec<(Range<usize>, Hsla)> {
    let mut spans = Vec::with_capacity(lines.len());
    let mut at = 0;
    for line in lines {
        let head = line.head_len();
        if line.is_severe() {
            let level = line.level_range();
            spans.push((at..at + level.start, colors.head));
            spans.push((at + level.start..at + level.end, colors.severe));
            spans.push((at + level.end..at + head, colors.head));
        } else {
            spans.push((at..at + head, colors.head));
        }
        at += line.text().len() + 1;
    }
    spans
}
