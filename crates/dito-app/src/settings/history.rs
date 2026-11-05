use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use dito_core::i18n::Tr;
use dito_core::prompts::Style;
use dito_core::store::{HistoryEntry, HistoryFilter, HistoryKind};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::query_state::{Finish, QueryState, SkeletonGate, Snapshot, apply_snapshot};
use super::{Ctx, watch_config};
use crate::assets::Ic;
use crate::services;
use crate::state::AppState;
use std::cell::RefCell;

use crate::theme::{Palette, Tint};
use crate::timefmt::{hhmm, local_day, now, ymd};
use crate::ui::{self, BtnKind, Cb, MenuItem, MenuTone, cbk, cbk_idx, hex_alpha, text};

type PageAction = Box<dyn Fn(&mut HistoryPage, &mut Context<HistoryPage>)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Period {
    All,
    Today,
    Week,
}

pub struct HistoryPage {
    st: Arc<AppState>,
    search: Entity<InputState>,
    query: String,
    period: Period,
    style: Option<Style>,
    select_open: bool,
    menu: Option<i64>,
    expanded: HashSet<i64>,
    row_focus: HashMap<i64, FocusHandle>,
    hovered_original: Option<i64>,
    busy: HashSet<i64>,
    /// Owned list snapshot: swapped wholesale by `loaded`, never read from the database in
    /// render. Shared as a handle so a render clones one Arc, not 500 rows of strings.
    entries: Arc<Vec<HistoryEntry>>,
    /// Last query failure, kept for logging/debugging — never rendered as UI copy.
    error: Option<anyhow::Error>,
    /// Load lifecycle: active gate, single-flight queries, generation checks.
    queries: QueryState,
    /// Distinguishes the poll task of the current activation from earlier, exited ones.
    poll_epoch: u64,
    /// Newest history id seen by the lightweight poll probe; only the probe writes it, so a
    /// change always means the database moved since the previous probe.
    last_seen_id: i64,
    placeholder_lang: dito_core::i18n::Lang,
    /// While true the list renders as shimmering placeholders: building the real rows lays out a
    /// a lot of text, and doing that on every frame of the nav-plate animation starves it. The
    /// skeleton keeps those frames cheap; the content swaps in once the plate has settled.
    skeleton: bool,
    /// Clears `skeleton` only after both the settle delay and the first query outcome.
    gate: SkeletonGate,
    /// Virtualized list state (visible rows only are laid out), held here so scrolling survives
    /// re-renders. `listed` tracks the item count last given to the state.
    list_state: ListState,
    listed: usize,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

impl HistoryPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);
        let lang = st.cfg.get().general.lang();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::HistorySearch)));
        ui::notify_on_focus(&search, window, cx);
        cx.subscribe_in(&search, window, |this: &mut Self, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.query = state.read(cx).value().to_string();
                this.changed(cx);
                cx.notify();
            }
        })
        .detach();
        let this = Self {
            motion: ui::MotionScope::new(),
            st,
            search,
            query: String::new(),
            period: Period::All,
            style: None,
            select_open: false,
            menu: None,
            expanded: HashSet::new(),
            row_focus: HashMap::new(),
            hovered_original: None,
            busy: HashSet::new(),
            entries: Arc::new(Vec::new()),
            error: None,
            queries: QueryState::new(),
            poll_epoch: 0,
            last_seen_id: 0,
            placeholder_lang: lang,
            skeleton: true,
            gate: SkeletonGate::new(),
            list_state: ListState::new(0, ListAlignment::Top, px(600.)),
            listed: 0,
        };
        // The list scrolls inside its own element without re-running this view, so the edge
        // fade overlays would freeze at their first value. Nudge the entity on scroll — weakly:
        // the handler must not keep the page alive once its window is gone.
        let entity = cx.entity().downgrade();
        this.list_state.set_scroll_handler(move |_, _, cx| {
            entity
                .update(cx, |this, cx| {
                    this.hovered_original = None;
                    cx.notify();
                })
                .ok();
        });
        this
    }

    /// Enter skeleton mode for one plate-settle window, then swap the real list in. The
    /// skeleton clears only when that window has elapsed AND the first query outcome (data
    /// or error) has landed, so a slow first load never flashes an empty list.
    pub fn enter(&mut self, cx: &mut Context<Self>) {
        self.skeleton = true;
        let entry = self.gate.enter();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(ui::GLASS_MS)).await;
            let _ = this.update(cx, |h, cx| {
                // A timer from an earlier visit must not clear this one's skeleton.
                if h.skeleton && h.gate.settle_elapsed(entry) {
                    h.skeleton = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn filter(&self) -> HistoryFilter {
        let off = dito_platform::local_utc_offset_secs();
        let since = match self.period {
            Period::All => None,
            Period::Today => Some(crate::timefmt::day_start(local_day(now(), off), off)),
            Period::Week => Some(now() - 7 * 86_400),
        };
        HistoryFilter { query: Some(self.query.clone()), since_ts: since, style: self.style, limit: Some(500) }
    }

    /// Same-input reload (poll tick, action completion, page entry). Collapses into the
    /// running query; its result still applies when it lands.
    fn load(&mut self, cx: &mut Context<Self>) {
        if let Some(ticket) = self.queries.request() {
            self.launch(ticket, cx);
        }
    }

    /// The inputs changed (search text, period, style): any result still running under the
    /// old inputs is stale and will be dropped when it lands.
    fn changed(&mut self, cx: &mut Context<Self>) {
        if let Some(ticket) = self.queries.restart() {
            self.launch(ticket, cx);
        }
    }

    fn launch(&mut self, ticket: u64, cx: &mut Context<Self>) {
        let st = self.st.clone();
        let filter = self.filter();
        cx.spawn(async move |this, cx| {
            let r = services::history_list(st, filter).await;
            let _ = this.update(cx, |t, cx| t.loaded(ticket, r, cx));
        })
        .detach();
    }

    fn loaded(&mut self, ticket: u64, result: anyhow::Result<Vec<HistoryEntry>>, cx: &mut Context<Self>) {
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

    fn apply(&mut self, result: anyhow::Result<Vec<HistoryEntry>>, cx: &mut Context<Self>) {
        match apply_snapshot(&mut self.entries, result.map(Arc::new)) {
            Snapshot::Failed(e) => {
                tracing::warn!("history list failed: {e:#}");
                self.error = Some(e);
            }
            Snapshot::Replaced => {
                cx.notify();
                self.error = None;
            }
            // Same rows: no re-render, matching the old probe-only poll that stayed quiet.
            Snapshot::Unchanged => self.error = None,
        }
        // Any first outcome — data or error — ends the skeleton once the settle window has
        // also elapsed; later outcomes find the skeleton already down and do nothing.
        if self.skeleton && self.gate.outcome_received() {
            self.skeleton = false;
            cx.notify();
        }
    }

    /// Page visibility lifecycle, driven by `Settings`. While inactive the page launches no
    /// queries and late results are ignored; entering loads at once and starts the poll.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        let was = self.queries.is_active();
        self.queries.set_active(active);
        if !active {
            self.hovered_original = None;
        }
        if active {
            self.load(cx);
            if !was {
                self.start_poll(cx);
            }
        }
    }

    /// New dictations arrive while the page is open: probe the newest id off the UI thread
    /// and reload the full list only when it changed. One poll task per activation; a
    /// superseded or inactive task exits at its next tick.
    fn start_poll(&mut self, cx: &mut Context<Self>) {
        self.poll_epoch += 1;
        let epoch = self.poll_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1200)).await;
                // Never launch a probe for a hidden page or a superseded task.
                let st = match this
                    .update(cx, |t, _| (t.poll_epoch == epoch && t.queries.is_active()).then(|| t.st.clone()))
                {
                    Ok(Some(st)) => st,
                    // Entity dropped, or superseded/inactive activation.
                    _ => break,
                };
                let probe = services::history_list(st, HistoryFilter { limit: Some(1), ..Default::default() }).await;
                let mut alive = false;
                let ok = this
                    .update(cx, |t, cx| {
                        alive = t.poll_epoch == epoch && t.queries.is_active();
                        if !alive {
                            return;
                        }
                        match probe {
                            Ok(v) => {
                                let newest = v.first().map(|e| e.id).unwrap_or(0);
                                if newest != t.last_seen_id {
                                    t.last_seen_id = newest;
                                    t.load(cx);
                                }
                            }
                            // Keep the last-good snapshot and id: a transient probe failure
                            // must not read as an emptied history or trigger a reload.
                            Err(e) => tracing::warn!("history poll probe failed: {e:#}"),
                        }
                    })
                    .is_ok();
                if !ok || !alive {
                    break;
                }
            }
        })
        .detach();
    }

    fn run_job<F, Fut>(&mut self, id: i64, cx: &mut Context<Self>, job: F)
    where
        F: FnOnce(Arc<AppState>, HistoryEntry) -> Fut + 'static,
        Fut: Future<Output = Result<(), String>> + 'static,
    {
        // Reserve before the first await: repeated clicks while the lookup runs must not
        // launch a second job for the same row.
        if !self.busy.insert(id) {
            return;
        }
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let entry = match services::history_entry(st.clone(), id).await {
                Ok(Some(e)) => e,
                other => {
                    if let Err(e) = other {
                        tracing::warn!("history action failed: {e:#}");
                    }
                    let _ = this.update(cx, |t, cx| {
                        t.busy.remove(&id);
                        cx.notify();
                    });
                    return;
                }
            };
            let r = job(st, entry).await;
            if let Err(e) = &r {
                tracing::warn!("history action failed: {e}");
            }
            let _ = this.update(cx, |t, cx| {
                t.busy.remove(&id);
                // Unbusy is visible regardless of what the follow-up reload returns.
                cx.notify();
                t.load(cx);
            });
        })
        .detach();
    }

    fn toggle_original(&mut self, id: i64, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        cx.notify();
    }

    fn copy(&self, text: &str) {
        if let Err(e) = dito_platform::insert::copy_to_clipboard(text) {
            tracing::warn!("copy failed: {e}");
        }
    }

    fn delete(&mut self, id: i64, cx: &mut Context<Self>) {
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = services::delete_history_entry(st, id).await {
                tracing::warn!("history delete failed: {e:#}");
            }
            let _ = this.update(cx, |t, cx| t.load(cx));
        })
        .detach();
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let dir = std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Downloads"))
            .unwrap_or_else(|| ".".into());
        let rx = cx.prompt_for_new_path(&dir, Some("dito-history.csv"));
        let st = self.st.clone();
        cx.spawn(async move |_, _| {
            if let Ok(Ok(Some(path))) = rx.await {
                let rows = services::history_list(st.clone(), HistoryFilter::default()).await.unwrap_or_default();
                let off = dito_platform::local_utc_offset_secs();
                let mut out = String::from("date,time,app,style,text,original\n");
                for e in rows {
                    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
                    out.push_str(&format!(
                        "{},{},{},{},{},{}\n",
                        ymd(local_day(e.ts, off)),
                        hhmm(e.ts, off),
                        q(&e.app),
                        e.style.label(),
                        q(e.final_text()),
                        q(&e.raw)
                    ));
                }
                let rt = st.rt.clone();
                match rt.spawn_blocking(move || std::fs::write(&path, out)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => tracing::warn!("export: {e}"),
                    Err(e) => tracing::warn!("export: {e}"),
                }
            }
        })
        .detach();
    }

    fn menu_for(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry) -> Div {
        let id = e.id;
        let close: Cb = cbk(cx, |this: &mut Self, _, cx| {
            this.menu = None;
            cx.notify();
        });
        let wrap = |cx: &mut Context<Self>, f: PageAction| -> Cb { cbk(cx, move |this: &mut Self, _, cx| f(this, cx)) };
        let mut items = vec![
            MenuItem {
                icon: Some(Ic::Mic),
                label: c.t(Tr::HistoryRecognizeAgain),
                tone: MenuTone::Default,
                on_click: wrap(cx, Box::new(move |t, cx| t.run_job(id, cx, services::recognize_again))),
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
            on_click: wrap(cx, Box::new(move |t, _| t.copy(&raw))),
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

    fn row(&self, c: &Ctx, cx: &mut Context<Self>, e: &HistoryEntry, off: i32, last: bool, window: &Window) -> Div {
        let p = &c.p;
        let id = e.id;
        let busy = self.busy.contains(&id);
        let more_open = self.menu == Some(id);

        let mut meta = div().flex().items_center().gap(px(10.)).child(
            // Fixed-width slot: times are always 00:00-shaped but the font is proportional, so
            // without this the tag after it starts at a different x on every row.
            div().w(px(38.)).flex_none().child(text(hhmm(e.ts, off), 13., FontWeight::NORMAL, p.muted)),
        );
        let mut body = div().w_full().flex().flex_col().gap(px(10.));
        let mut actions = div().flex().items_center().gap(px(8.)).flex_none();

        let retry_btn = |label: SharedString, ic: Ic, cx: &mut Context<Self>| {
            if busy {
                text(c.t(Tr::MenubarWorking), 13., FontWeight::NORMAL, p.muted).into_any_element()
            } else {
                ui::button(
                    SharedString::from(format!("retry-{id}")),
                    p,
                    &label,
                    BtnKind::Secondary,
                    Some(ic),
                    cbk(cx, move |t: &mut Self, _, cx| t.run_job(id, cx, services::recognize_again)),
                )
                .into_any_element()
            }
        };

        match e.kind {
            HistoryKind::Dictation => {
                meta = meta.child(ui::tag(p, ui::style_label(c.lang, e.style), Tint::Blue));
                body = body.child(
                    text(e.final_text().to_string(), 14., FontWeight::NORMAL, p.text).line_height(relative(1.6)),
                );
                if expandable(e) {
                    let raw = e.raw.clone();
                    let original = c.t(Tr::HistoryOriginal);
                    let p = *p;
                    let open = self.expanded.contains(&id);
                    body = body.gap(px(0.)).child(OriginalDisclosure { id, raw, original, p, open });
                }
                let t = e.final_text().to_string();
                actions = actions.child(ui::icon_button(
                    SharedString::from(format!("copy-{id}")),
                    p,
                    Ic::Copy,
                    false,
                    cbk(cx, move |this: &mut Self, _, _| this.copy(&t)),
                ));
            }
            HistoryKind::Cancelled => {
                meta = meta.child(ui::tag(p, c.t(Tr::ModelsCancel), Tint::Warning)).child(text(
                    format!("{}:{:02}", e.duration_ms / 60_000, e.duration_ms / 1000 % 60),
                    12.,
                    FontWeight::NORMAL,
                    p.muted,
                ));
                body = body.child(text(c.t(Tr::HistoryCancelled), 14., FontWeight::NORMAL, p.muted));
                actions = actions.child(retry_btn(c.t(Tr::HistoryRecognizeAgain), Ic::Mic, cx));
            }
            HistoryKind::Failed => {
                meta = meta.child(ui::tag(p, c.t(Tr::ModelsFailed), Tint::Danger));
                body = body.child(text(
                    ui::error_line(c.lang, e.error.as_deref().unwrap_or_default()),
                    14.,
                    FontWeight::NORMAL,
                    p.muted,
                ));
                actions = actions.child(retry_btn(c.t(Tr::HistoryRetry), Ic::Refresh, cx));
            }
        }
        let more = {
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
        };
        actions = actions.child(more);

        let can_expand = expandable(e);
        let body_id = ElementId::from(("hist-body", id as usize));
        let mut trigger =
            div().id(body_id.clone()).flex_1().min_w(px(0.)).flex().flex_col().gap(px(10.)).child(meta).child(body);
        if can_expand {
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
        if can_expand {
            let focused =
                window.last_input_was_keyboard() && self.row_focus.get(&id).is_some_and(|f| f.is_focused(window));
            row = row.child(
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
                .with_priority(2),
            );
        }
        if !last {
            row = row.border_b_1().border_color(p.border);
        }
        row
    }
}

impl HistoryPage {
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
        self.hovered_original = None;
    }

    /// The page's scope, for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for HistoryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || {
            let content = self.view(window, cx).into_any_element();
            div()
                .id("history-page")
                .size_full()
                .flex()
                .flex_col()
                .on_hover(cx.listener(|this, over: &bool, _, cx| {
                    if !over && this.hovered_original.take().is_some() {
                        cx.notify();
                    }
                }))
                .child(content)
        })
    }
}

impl HistoryPage {
    /// One shimmering placeholder row, sized against the real rows (~89 px pitch): two text bars
    /// plus a short meta bar, action-button squares on the right, and a gloss band sweeping
    /// across on a loop.
    fn skeleton_row(p: &Palette, id: usize) -> Div {
        let mut bars = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(10.));
        for (w, h) in [(0.95, 16.), (0.68, 16.), (0.22, 11.)] {
            bars = bars.child(div().w(relative(w)).h(px(h)).rounded(px(h / 2.)).bg(p.seg));
        }
        let glow = hex_alpha(gpui_kit::hsla(0., 0., 1., 1.), if p.dark { 0.07 } else { 0.55 });
        let clear = gpui_kit::hsla(0., 0., 1., 0.);
        let band = div()
            .absolute()
            .top_0()
            .h_full()
            .w(relative(0.36))
            .child(div().w(relative(0.5)).h_full().bg(linear_gradient(
                90.,
                linear_color_stop(clear, 0.),
                linear_color_stop(glow, 1.),
            )))
            .child(div().w(relative(0.5)).h_full().bg(linear_gradient(
                90.,
                linear_color_stop(glow, 0.),
                linear_color_stop(clear, 1.),
            )));
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(18.))
            .relative()
            .child(bars)
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(div().size(px(28.)).rounded(px(8.)).bg(p.seg))
                    .child(div().size(px(28.)).rounded_full().bg(p.seg)),
            )
            .child(div().absolute().top_0().left_0().size_full().overflow_hidden().rounded(px(10.)).child(
                band.with_animation(
                    (ElementId::from(("skeleton", id)), "sweep"),
                    Animation::new(Duration::from_millis(1300)).repeat(),
                    |el, t| el.left(relative(t * 1.4 - 0.4)),
                ),
            ))
    }

    fn skeleton_heading(p: &Palette) -> Div {
        div().w(px(44.)).h(px(16.)).rounded(px(8.)).bg(p.seg)
    }

    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        if self.placeholder_lang != c.lang {
            self.placeholder_lang = c.lang;
            self.search.update(cx, |s, cx| s.set_placeholder(c.t(Tr::HistorySearch), window, cx));
        }
        let off = dito_platform::local_utc_offset_secs();
        // Handle clone (one Arc), not a copy of every row: the snapshot is swapped wholesale by
        // `loaded`; render never touches the database.
        let snapshot = self.entries.clone();
        let entries: &[HistoryEntry] = if self.skeleton { &[] } else { &snapshot };

        self.row_focus.retain(|id, _| snapshot.iter().any(|e| e.id == *id && expandable(e)));
        for e in entries.iter().filter(|e| expandable(e)) {
            self.row_focus.entry(e.id).or_insert_with(|| {
                let focus = cx.focus_handle().tab_stop(true);
                cx.on_focus_in(&focus, window, |_, _, cx| cx.notify()).detach();
                cx.on_focus_out(&focus, window, |_, _, _, cx| cx.notify()).detach();
                focus
            });
        }

        let period_seg = ui::seg(
            "period",
            &p,
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
            &p,
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

        let toolbar = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(div().flex_1().child(ui::text_input(&p, &self.search, Some(Ic::Search))))
            .child(period_seg)
            .child(style_select);

        let actions = ui::button(
            "export-history",
            &p,
            &c.t(Tr::DictionaryExport),
            BtnKind::Secondary,
            Some(Ic::Download),
            cbk(cx, |this: &mut Self, _, cx| this.export(cx)),
        )
        .into_any_element();
        let mut column = div().w_full().flex().flex_col().gap(px(20.));
        if self.skeleton {
            // Toolbar is live already; only the heavy rows defer. Two day groups keep the shape of
            // the real list so the swap does not reflow the page.
            for group in 0..2u8 {
                let rows = if group == 0 { 3 } else { 2 };
                let mut card = ui::card(&p).flex_col().gap(px(32.)).p(px(18.));
                for r in 0..rows {
                    card = card.child(Self::skeleton_row(&p, group as usize * 4 + r as usize));
                }
                column = column
                    .child(div().w_full().flex().flex_col().gap(px(12.)).child(Self::skeleton_heading(&p)).child(card));
            }
            return super::page_fill(
                p,
                "history",
                c.t(Tr::NavHistory),
                Some(actions),
                div().w_full().flex().flex_col().gap(px(24.)).child(toolbar).child(column),
            );
        }
        let empty = entries.is_empty().then(|| {
            div()
                .w_full()
                .py(px(40.))
                .flex()
                .justify_center()
                .child(ui::muted12(&p, c.t(Tr::HistoryNoHistoryYet)).text_size(px(14.)))
                .into_any_element()
        });
        let today = local_day(now(), off);
        let mut i = 0;
        // Flatten to one element per heading/row and hand them to the virtualized list: only the
        // visible slice is laid out and painted, so scrolling stays smooth however long the log
        // grows. Element building here is cheap struct construction; text shaping happens per
        // visible item only. The Rc slot hands each element to `list` exactly once per frame.
        let mut items: Vec<Option<AnyElement>> = empty.map(|e| vec![Some(e)]).unwrap_or_default();
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
                    // 20 px above a group (the old list gap), 12 px between the heading and
                    // its card (the old group's internal gap), both lost when headings and
                    // cards became separate list items.
                    .pt(px(pad_top))
                    .pb(px(12.))
                    .child(text(heading, 16., FontWeight::BOLD, p.text))
                    .into_any_element(),
            ));
            let mut card = ui::card(&p);
            for (k, e) in entries[i..end].iter().enumerate() {
                card = card.child(self.row(&c, cx, e, off, i + k + 1 == end, window));
            }
            items.push(Some(card.into_any_element()));
            i = end;
        }
        let n = items.len();
        if self.hovered_original.is_some_and(|id| !entries.iter().any(|e| e.id == id && expandable(e))) {
            self.hovered_original = None;
        }
        if n != self.listed {
            self.listed = n;
            self.list_state.reset(n);
        }
        let items = RefCell::new(items);
        let vlist = list(self.list_state.clone(), move |ix, _, _| {
            items.borrow_mut().get_mut(ix).and_then(|slot| slot.take()).unwrap_or_else(|| {
                div().h(px(80.)).into_any_element() // fallback: state counted an item we dropped
            })
        })
        .flex_1()
        .min_h(px(0.));

        // Edge fades: where content continues above/below, dissolve it into the page so the cut
        // reads as "scrollable". Pure background (no id, no listeners) — never hit-tested.
        const FADE: f32 = 28.;
        let scrolled = -f32::from(self.list_state.scroll_px_offset_for_scrollbar().y);
        let max_scroll = f32::from(self.list_state.max_offset_for_scrollbar().y);
        let top_a = (scrolled / FADE).clamp(0., 1.);
        let bottom_a = ((max_scroll - scrolled) / FADE).clamp(0., 1.);

        super::page_fill(
            p,
            "history",
            c.t(Tr::NavHistory),
            Some(actions),
            div().w_full().flex_1().min_h(px(0.)).flex().flex_col().gap(px(24.)).child(toolbar).child(
                // Positioning context = the list viewport only, so the fades hug the scrolled
                // content's edges instead of the toolbar's.
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .child(vlist)
                    .when(top_a > 0.001, |d| {
                        d.child(div().absolute().top_0().left_0().right_0().h(px(FADE)).opacity(top_a).bg(
                            linear_gradient(
                                180.,
                                linear_color_stop(hex_alpha(p.bg, 1.), 0.),
                                linear_color_stop(hex_alpha(p.bg, 0.), 1.),
                            ),
                        ))
                    })
                    .when(bottom_a > 0.001, |d| {
                        d.child(div().absolute().bottom_0().left_0().right_0().h(px(FADE)).opacity(bottom_a).bg(
                            linear_gradient(
                                0.,
                                linear_color_stop(hex_alpha(p.bg, 1.), 0.),
                                linear_color_stop(hex_alpha(p.bg, 0.), 1.),
                            ),
                        ))
                    }),
            ),
        )
    }
}

/// A late leave from the previous row must not clear the new row's hint.
fn update_original_hover(current: &mut Option<i64>, id: i64, over: bool) -> bool {
    let next = if over { Some(id) } else { current.filter(|owner| *owner != id) };
    let changed = *current != next;
    *current = next;
    changed
}

/// Only successful, polished records with original text offer disclosure.
fn expandable(entry: &HistoryEntry) -> bool {
    entry.kind == HistoryKind::Dictation && entry.polished.is_some() && !entry.raw.is_empty()
}

/// Keep one measured reveal mounted throughout animation and rest. Wrapping it
/// in `tweened` changed its ancestor identity at settle, discarded its measured
/// height and briefly laid it out at zero height.
#[derive(IntoElement)]
struct OriginalDisclosure {
    id: i64,
    raw: String,
    original: SharedString,
    p: Palette,
    open: bool,
}

impl RenderOnce for OriginalDisclosure {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let progress = gpui_kit::base::transition(
            ElementId::from(("history-original-progress", self.id as usize)),
            if self.open { 1_f32 } else { 0_f32 },
            gpui_kit::base::Transition::new(Duration::from_millis(if dito_platform::system::reduce_motion() {
                0
            } else {
                200
            }))
            .ease(|t| t * t * (3. - 2. * t)),
            window,
            cx,
        );
        gpui_kit::base::Collapsible::new().reveal(("history-content", self.id as usize), progress).content(
            div().pt(px(10.)).child(
                div()
                    .w_full()
                    .px(px(16.))
                    .py(px(12.))
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .rounded(px(12.))
                    .bg(self.p.sub)
                    .opacity(progress)
                    .child(text(self.original, 12., FontWeight::MEDIUM, self.p.muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(text(self.raw, 12., FontWeight::NORMAL, self.p.muted).line_height(relative(1.6))),
                    ),
            ),
        )
    }
}

#[cfg(all(test, feature = "screenshots"))]
mod disclosure_tests {
    use super::*;
    use ::core::prelude::v1::test;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Probe {
        open: Rc<Cell<bool>>,
        height: Rc<Cell<f32>>,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let height = self.height.clone();
            div().w(px(400.)).flex().flex_col().child(
                div()
                    .relative()
                    .child(OriginalDisclosure {
                        id: 1,
                        raw: "A long original transcription that wraps onto several lines. ".repeat(8),
                        original: "Original".into(),
                        p: Palette::light(),
                        open: self.open.get(),
                    })
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                height.set(f32::from(bounds.size.height));
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
        }
    }

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

    #[gpui_kit::test]
    fn original_keeps_its_height_after_settling_and_reversing(cx: &mut TestAppContext) {
        let open = Rc::new(Cell::new(false));
        let height = Rc::new(Cell::new(0.));
        let window = cx.open_window(size(px(500.), px(600.)), {
            let open = open.clone();
            let height = height.clone();
            move |_, _| Probe { open, height }
        });
        let draw = |cx: &mut TestAppContext| {
            window.update(cx, |_, window, _| window.refresh()).unwrap();
            cx.run_until_parked();
        };
        draw(cx);
        open.set(true);
        draw(cx);
        cx.executor().advance_clock(Duration::from_millis(250));
        draw(cx);
        let expanded = height.get();
        assert!(expanded > 30., "long original must have a measured expanded height");
        for _ in 0..4 {
            cx.executor().advance_clock(Duration::from_millis(100));
            draw(cx);
            assert_eq!(height.get(), expanded, "settling must not discard the measured height");
        }
        open.set(false);
        draw(cx);
        cx.executor().advance_clock(Duration::from_millis(80));
        draw(cx);
        let partial = height.get();
        assert!(partial > 0. && partial < expanded);
        open.set(true);
        draw(cx);
        assert!((height.get() - partial).abs() < 0.1, "reversing must preserve the current height");
        cx.executor().advance_clock(Duration::from_millis(250));
        draw(cx);
        assert_eq!(height.get(), expanded);
    }
}
