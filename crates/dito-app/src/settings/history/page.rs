use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use dito_core::i18n::Tr;
use dito_core::prompts::Style;
use dito_core::store::{HistoryEntry, HistoryFilter};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::services;
use crate::settings::page_query::{PageQuery, QueryPage};
use crate::settings::query_state::SkeletonGate;
use crate::state::AppState;
use crate::timefmt::{local_day, now};
use crate::ui::{self, InputEvent, InputState};

const POLL_INTERVAL: Duration = Duration::from_millis(1200);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Period {
    All,
    Today,
    Week,
}

pub struct HistoryPage {
    pub(super) st: Arc<AppState>,
    pub(super) search: Entity<InputState>,
    pub(super) query: String,
    pub(super) period: Period,
    pub(super) style: Option<Style>,
    pub(super) select_open: bool,
    pub(super) menu: Option<i64>,
    pub(super) expanded: HashSet<i64>,
    pub(super) row_focus: HashMap<i64, FocusHandle>,
    pub(super) hovered_original: Option<i64>,
    pub(super) busy: HashSet<i64>,
    /// Owned list snapshot, never read from the database in render. Shared as a handle so a
    /// render clones one Arc, not 500 rows of strings.
    pub(super) list: PageQuery<Arc<Vec<HistoryEntry>>>,
    /// Distinguishes the poll task of the current activation from earlier, exited ones.
    pub(super) poll_epoch: u64,
    /// Replacing it cancels the previous loop.
    _poll: Option<Task<()>>,
    /// Newest history id seen by the lightweight poll probe; only the probe writes it, so a
    /// change always means the database moved since the previous probe.
    pub(super) last_seen_id: i64,
    pub(super) placeholder_lang: dito_core::i18n::Lang,
    /// While true the list renders as shimmering placeholders: building the real rows lays out a lot of text, and doing
    /// that on every frame of the nav-plate animation starves it. The skeleton keeps those frames cheap; the content
    /// swaps in once the plate has settled.
    pub(super) skeleton: bool,
    pub(super) gate: SkeletonGate,
    /// Held here so scrolling survives re-renders. `listed` tracks the item count last given to the state.
    pub(super) list_state: ListState,
    pub(super) listed: usize,
    pub(super) motion: ui::MotionScope,
}

impl QueryPage for HistoryPage {
    type Data = Arc<Vec<HistoryEntry>>;

    fn page_query(&mut self) -> &mut PageQuery<Self::Data> {
        &mut self.list
    }

    fn fetch(&self) -> impl Future<Output = anyhow::Result<Self::Data>> + 'static {
        let list = services::history_list(self.st.clone(), self.filter());
        async move { list.await.map(Arc::new) }
    }

    fn applied(&mut self, cx: &mut Context<Self>) {
        // Data or error, either ends the skeleton once the settle window has also elapsed.
        if self.skeleton && self.gate.outcome_received() {
            self.skeleton = false;
            cx.notify();
        }
    }
}

impl HistoryPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A config change can add or remove a configured app: drop a filter whose app is gone, and reload so the
        // labels and the Other bucket follow the new list.
        ui::observe(st.cfg.subscribe(), cx, |this: &mut Self, cfg, cx| {
            if let Some(style) = &this.style
                && !cfg.prompts.profile_enabled(style)
            {
                this.style = None;
            }
            this.changed(cx);
            cx.notify();
        });
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
            list: PageQuery::new("history list", Arc::new(Vec::new())),
            poll_epoch: 0,
            _poll: None,
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

    /// The skeleton clears only when the settle window has elapsed AND the first query outcome (data or error) has
    /// landed, so a slow first load never flashes an empty list.
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

    /// Reads as All styles when the selected app is no longer configured, matching what the picker shows.
    pub(super) fn selected_style(&self) -> Option<Style> {
        self.style.as_ref().filter(|style| self.st.cfg.get().prompts.profile_enabled(style)).cloned()
    }

    fn filter(&self) -> HistoryFilter {
        let off = dito_platform::local_utc_offset_secs();
        let since = match self.period {
            Period::All => None,
            Period::Today => Some(crate::timefmt::day_start(local_day(now(), off), off)),
            Period::Week => Some(now() - 7 * 86_400),
        };
        let configured_apps =
            self.st.cfg.get().prompts.applications.iter().map(|(id, name)| (name.clone(), id.clone())).collect();
        HistoryFilter {
            query: Some(self.query.clone()),
            since_ts: since,
            style: self.selected_style(),
            configured_apps,
            disabled_builtin_styles: self.st.cfg.get().prompts.disabled_builtin_styles.clone(),
            limit: Some(500),
        }
    }

    /// Driven by `Settings`. While inactive the page launches no queries and late results are ignored.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        let was = self.list.is_active();
        self.list.set_active(active);
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

    /// One poll task per activation; a superseded or inactive task exits at its next tick.
    fn start_poll(&mut self, cx: &mut Context<Self>) {
        self.poll_epoch += 1;
        let epoch = self.poll_epoch;
        self._poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let Ok(Some(st)) = this.update(cx, |t, _| t.polls(epoch).then(|| t.st.clone())) else { break };
                let probe = services::history_list(st, HistoryFilter { limit: Some(1), ..Default::default() }).await;
                if !this.update(cx, |t, cx| t.apply_probe(epoch, probe, cx)).unwrap_or(false) {
                    break;
                }
            }
        }));
    }

    fn polls(&self, epoch: u64) -> bool {
        self.poll_epoch == epoch && self.list.is_active()
    }

    /// Whether the poll task of `epoch` should keep running.
    fn apply_probe(&mut self, epoch: u64, probe: anyhow::Result<Vec<HistoryEntry>>, cx: &mut Context<Self>) -> bool {
        if !self.polls(epoch) {
            return false;
        }
        match probe {
            Ok(entries) => {
                let newest = entries.first().map(|e| e.id).unwrap_or(0);
                if newest != self.last_seen_id {
                    self.last_seen_id = newest;
                    self.load(cx);
                }
            }
            // Keep the last-good snapshot and id: a transient probe failure
            // must not read as an emptied history or trigger a reload.
            Err(e) => tracing::warn!("history poll probe failed: {e:#}"),
        }
        true
    }
}

impl HistoryPage {
    pub(in crate::settings) fn reset_motion(&mut self) {
        self.motion.reset();
        self.hovered_original = None;
    }

    #[cfg(feature = "screenshots")]
    pub(in crate::settings) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for HistoryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Own scope: after a page switch the controls mount statically instead of sliding to values remembered from an
        // earlier visit.
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
