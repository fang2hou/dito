use dito_core::i18n::Tr;
use dito_core::prompts::AppTarget;
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
        let cfg = self.st.cfg.get();
        let mut targets: Vec<AppTarget> =
            AppTarget::all().into_iter().filter(|target| cfg.prompts.target_enabled(target)).collect();
        let mut apps: Vec<(&str, &str)> =
            cfg.prompts.applications.iter().map(|(id, name)| (name.as_str(), id.as_str())).collect();
        apps.sort_by_cached_key(|(name, _)| name.to_lowercase());
        apps.dedup_by(|a, b| a.0 == b.0);
        targets.extend(apps.iter().map(|(_, id)| AppTarget::Application((*id).to_owned())));
        let target_opts: Vec<SharedString> = std::iter::once(c.t(Tr::HistoryAllApps))
            .chain(targets.iter().map(|t| ui::history_target_label(c.lang, &cfg.prompts, t, "")))
            .collect();
        let target_sel =
            self.selected_target().and_then(|t| targets.iter().position(|x| *x == t)).map(|i| i + 1).unwrap_or(0);
        let picked = targets.clone();
        let target_select = ui::select(
            "app-filter",
            p,
            &target_opts,
            target_sel,
            self.select_open,
            Some(240.),
            cbk(cx, |this: &mut Self, _, cx| {
                this.select_open = !this.select_open;
                cx.notify();
            }),
            cbk_idx(cx, move |this: &mut Self, i, _, cx| {
                this.target = if i == 0 { None } else { Some(picked[i - 1].clone()) };
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
            .child(target_select)
    }
}
