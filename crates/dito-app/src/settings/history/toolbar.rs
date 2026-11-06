use dito_core::i18n::Tr;
use dito_core::prompts::Style;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::{HistoryPage, Period};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::settings::page_query::QueryPage;
use crate::ui::{self, cbk, cbk_idx};

impl HistoryPage {
    pub(super) fn toolbar(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let period_seg = ui::seg(
            "period",
            p,
            &[c.t(Tr::HistoryAll), c.t(Tr::HistoryToday), c.t(Tr::HistoryThisWeek)],
            match self.period {
                Period::All => 0,
                Period::Today => 1,
                Period::Week => 2,
            },
            cbk_idx(cx, |this: &mut Self, i, _, cx| {
                this.period = [Period::All, Period::Today, Period::Week][i];
                this.changed(cx);
                cx.notify();
            }),
        );
        let style_opts: Vec<SharedString> = std::iter::once(c.t(Tr::HistoryAllStyles))
            .chain(Style::ALL.iter().map(|s| ui::style_label(c.lang, *s)))
            .collect();
        let style_sel = self.style.and_then(|s| Style::ALL.iter().position(|x| *x == s)).map(|i| i + 1).unwrap_or(0);
        let style_select = ui::select(
            "style-filter",
            p,
            &style_opts,
            style_sel,
            self.select_open,
            Some(240.),
            cbk(cx, |this: &mut Self, _, cx| {
                this.select_open = !this.select_open;
                cx.notify();
            }),
            cbk_idx(cx, |this: &mut Self, i, _, cx| {
                this.style = if i == 0 { None } else { Some(Style::ALL[i - 1]) };
                this.changed(cx);
                cx.notify();
            }),
        );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(div().flex_1().child(ui::text_input(p, &self.search, Some(Ic::Search))))
            .child(period_seg)
            .child(style_select)
    }
}
