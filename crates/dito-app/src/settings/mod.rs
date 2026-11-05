//! The settings window: sidebar shell plus one view per page.

use std::{cell::Cell, rc::Rc, sync::Arc};

use dito_core::config::{EngineConfig, Source};
use dito_core::health::{EngineStatus, Health};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::state::{AppGlobal, AppState};
use crate::theme::{self, Palette};
use crate::ui::{self, text};

pub(crate) mod asr_editor;
mod asr_highlight;
mod dictionary;
mod general;
mod history;
mod hud_slider;
mod models;
mod prompts;
mod query_state;
mod stats;
mod status;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    General,
    Models,
    Dictionary,
    Prompts,
    History,
    Stats,
    Status,
}

impl Page {
    #[cfg(feature = "screenshots")]
    pub const ALL: [Page; 7] =
        [Self::General, Self::Models, Self::Dictionary, Self::Prompts, Self::History, Self::Stats, Self::Status];

    pub fn label(self) -> Tr {
        match self {
            Self::General => Tr::NavGeneral,
            Self::Models => Tr::NavModels,
            Self::Dictionary => Tr::NavDictionary,
            Self::Prompts => Tr::NavPrompts,
            Self::History => Tr::NavHistory,
            Self::Stats => Tr::NavStats,
            Self::Status => Tr::NavStatus,
        }
    }

    pub fn english(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Models => "Models",
            Self::Dictionary => "Dictionary",
            Self::Prompts => "Prompts",
            Self::History => "History",
            Self::Stats => "Stats",
            Self::Status => "Status",
        }
    }

    /// Every page in sidebar order.
    const NAV: [Page; 7] =
        [Self::General, Self::Models, Self::Dictionary, Self::Prompts, Self::History, Self::Stats, Self::Status];

    /// Position in the sidebar, top to bottom.
    fn nav_index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Models => 1,
            Self::Dictionary => 2,
            Self::Prompts => 3,
            Self::History => 4,
            Self::Stats => 5,
            Self::Status => 6,
        }
    }

    fn icon(self) -> Ic {
        match self {
            Self::General => Ic::Sliders,
            Self::Models => Ic::Cpu,
            Self::Dictionary => Ic::BookOpen,
            Self::Prompts => Ic::Sparkles,
            Self::History => Ic::Clock,
            Self::Stats => Ic::Chart,
            Self::Status => Ic::Activity,
        }
    }
}

const NAV_H: f32 = 44.;
const NAV_SECTION_H: f32 = 36.;
const NAV_GAP: f32 = 4.;
/// Below this width the sidebar collapses into an icon rail on its own — never a user toggle:
/// small windows reserve more space for content. Expanding again needs RAIL_EXPAND so dragging across
/// the boundary cannot flicker. Row positions relative to the nav column are identical in both
/// modes, so the nav plate and hover state survive the collapse without replaying.
const RAIL_COLLAPSE: f32 = 1024.;
const RAIL_EXPAND: f32 = 1088.;
const RAIL_MS: u64 = 180;
const RAIL_WIDTH: f32 = 76.;
const RAIL_PADDING: f32 = (RAIL_WIDTH - 52.) / 2.;

fn traffic_light_position(rail: bool) -> Point<Pixels> {
    // Keep the close button 8 px from the rounded window corner. The compact
    // rail is slightly narrower than the fully symmetric button-group envelope.
    let inset = px(if rail { 8. } else { 20. });
    point(inset, inset)
}

const fn sidebar_top_height(rail: bool) -> f32 {
    // Leave another 6 px below the native buttons before the compact D mark.
    if rail { 36. } else { 54. }
}

/// Top of the collapsed nav column: drag strip + brand slot (51) + two gaps (4 each).
const NAV_COL_Y: f32 = sidebar_top_height(true) + 51. + 2. * NAV_GAP;

/// Top of the sidebar item for `page` inside the nav column: two section headers, items in order.
fn nav_y(page: Page) -> f32 {
    let step = NAV_H + NAV_GAP;
    let header = NAV_SECTION_H + NAV_GAP;
    match page {
        Page::General | Page::Models | Page::Dictionary | Page::Prompts => header + page.nav_index() as f32 * step,
        _ => header + 4. * step + header + (page.nav_index() - 4) as f32 * step,
    }
}

/// Everything a page needs to paint itself: palette, language and the services.
pub struct Ctx {
    pub p: Palette,
    pub lang: Lang,
}

impl Ctx {
    pub fn new(st: &Arc<AppState>, cx: &App) -> Self {
        let cfg = st.cfg.get();
        let lang = cfg.general.lang();
        ui::set_ui_language(lang);
        Self { p: theme::palette(cfg.general.appearance, cx), lang }
    }

    pub fn t(&self, message: Tr) -> SharedString {
        SharedString::from(self.lang.tr(message))
    }
}

/// Re-render `view` whenever the config changes.
pub fn watch_config<V: 'static>(st: &Arc<AppState>, cx: &mut Context<V>) {
    let mut rx = st.cfg.subscribe();
    cx.spawn(async move |this, cx| {
        while rx.changed().await.is_ok() {
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// A role stays visible even when its engine is disabled; disabled rows have no live status.
struct HealthRow {
    role: &'static str,
    name: SharedString,
    status: Option<EngineStatus>,
}

const HEALTH_RADIUS: f32 = 23.;
const HEALTH_PADDING: f32 = 12.;
const HEALTH_ROW_HEIGHT: f32 = 22.;
const HEALTH_ROW_GAP: f32 = 13.;
const HEALTH_HEIGHT: f32 = HEALTH_PADDING * 2. + HEALTH_ROW_HEIGHT * 3. + HEALTH_ROW_GAP * 2.;
const HEALTH_MS: u64 = 320;

pub struct Settings {
    st: Arc<AppState>,
    page: Page,
    /// Sidebar is collapsed to the compact icon rail (auto below RAIL_COLLAPSE, with hysteresis).
    rail: bool,
    health_dot_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The persistent shell scope: the sidebar's nav plate and hover live here for the window's
    /// whole life; page navigation resets page scopes but never this one.
    motion: ui::MotionScope,
    general: Entity<general::GeneralPage>,
    models: Entity<models::ModelsPage>,
    dictionary: Entity<dictionary::DictionaryPage>,
    prompts: Entity<prompts::PromptsPage>,
    history: Entity<history::HistoryPage>,
    stats: Entity<stats::StatsPage>,
    status: Entity<status::StatusPage>,
}

impl Settings {
    #[cfg(feature = "screenshots")]
    pub fn health_hover_probe(&self) -> (Option<Bounds<Pixels>>, bool) {
        (self.health_dot_bounds.get(), Self::health_hovered(&self.motion.handle()))
    }

    #[cfg(feature = "screenshots")]
    pub fn set_hud_appearance(&self, appearance: dito_core::config::Appearance, cx: &mut App) {
        self.general.update(cx, |general, cx| general.set_hud_appearance(appearance, cx));
    }

    #[cfg(feature = "screenshots")]
    pub fn hud_preview(&self, cx: &App) -> Option<Entity<crate::hud_style_preview::StylePreview>> {
        self.general.read(cx).hud_preview(cx)
    }

    #[cfg(feature = "screenshots")]
    pub fn hud_height_bounds(&self, cx: &App) -> Bounds<Pixels> {
        self.general.read(cx).hud_height_bounds(cx)
    }

    pub fn new(st: Arc<AppState>, page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);
        let s = &st;
        let this = Self {
            general: cx.new(|cx| general::GeneralPage::new(s.clone(), cx)),
            models: cx.new(|cx| models::ModelsPage::new(s.clone(), window, cx)),
            dictionary: cx.new(|cx| dictionary::DictionaryPage::new(s.clone(), window, cx)),
            prompts: cx.new(|cx| prompts::PromptsPage::new(s.clone(), window, cx)),
            history: cx.new(|cx| history::HistoryPage::new(s.clone(), window, cx)),
            stats: cx.new(|cx| stats::StatsPage::new(s.clone(), cx)),
            status: cx.new(|cx| status::StatusPage::new(s.clone(), window, cx)),
            st,
            page,
            rail: false,
            health_dot_bounds: Rc::new(Cell::new(None)),
            motion: ui::MotionScope::new(),
        };
        // The modal is painted by the window shell, above the sidebar as well as the page.
        cx.subscribe(&this.models, |_, _, _: &models::ModalChanged, cx| cx.notify()).detach();
        this.activate(page, cx);
        this
    }

    /// Start the shown page's data lifecycle (history/stats load their snapshots on entry).
    fn activate(&self, page: Page, cx: &mut Context<Self>) {
        match page {
            Page::History => self.history.update(cx, |h, cx| {
                // Heavy list: skeleton it while the nav plate settles so its frames stay smooth.
                h.enter(cx);
                h.set_active(true, cx);
            }),
            Page::Stats => self.stats.update(cx, |s, cx| s.set_active(true, cx)),
            _ => {}
        }
    }

    /// Stop the left page's data lifecycle: no further queries, late results ignored.
    fn deactivate(&self, page: Page, cx: &mut Context<Self>) {
        match page {
            Page::History => self.history.update(cx, |h, cx| h.set_active(false, cx)),
            Page::Stats => self.stats.update(cx, |s, cx| s.set_active(false, cx)),
            _ => {}
        }
    }

    #[cfg(feature = "screenshots")]
    pub fn open_general_select(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.general.update(cx, |g, cx| g.open_select(id, cx));
    }

    pub fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page == page {
            return;
        }
        self.general.update(cx, |general, cx| general.cancel_hud_preview(cx));
        self.deactivate(self.page, cx);
        // The leaving page's transient state dies with its visit: nothing of it may replay
        // if the user comes back.
        self.reset_page_motion(self.page, cx);
        self.page = page;
        // The entered page's controls mount with fresh motion state: nothing may slide or replay
        // values remembered from an earlier visit. The shell (sidebar) scope is never reset here.
        self.reset_page_motion(page, cx);
        // Data lifecycle follows visibility: entering a page loads it, leaving stops its queries.
        self.activate(page, cx);
        cx.notify();
    }

    /// Reset one page's transient motion state (its controls mount statically on the next visit).
    fn reset_page_motion(&self, page: Page, cx: &mut Context<Self>) {
        match page {
            Page::General => self.general.update(cx, |g, _| g.reset_motion()),
            Page::Models => self.models.update(cx, |m, _| m.reset_motion()),
            Page::Dictionary => self.dictionary.update(cx, |d, _| d.reset_motion()),
            Page::Prompts => self.prompts.update(cx, |p, _| p.reset_motion()),
            Page::History => self.history.update(cx, |h, _| h.reset_motion()),
            Page::Stats => self.stats.update(cx, |s, _| s.reset_motion()),
            Page::Status => self.status.update(cx, |s, _| s.reset_motion()),
        }
    }

    /// The settings window closed: stop the shown page's data lifecycle and invalidate every
    /// transient scope (shell and pages), so handles still held elsewhere (tests, screenshots)
    /// read static defaults instead of stale motion state.
    fn on_window_closed(&mut self, cx: &mut Context<Self>) {
        self.deactivate(self.page, cx);
        self.motion.reset();
        for page in Page::NAV {
            self.reset_page_motion(page, cx);
        }
    }

    /// One row of the health tooltip: the engine's display name and its live status.
    #[cfg(feature = "screenshots")]
    pub fn preview_models(
        &self,
        polish: bool,
        editor: Option<bool>,
        help: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.models.update(cx, |m, cx| m.preview(polish, editor, help, window, cx));
    }

    fn health_name(lang: Lang, e: &EngineConfig, models: &dito_local::ModelManager) -> SharedString {
        match e.source {
            Source::Provider => SharedString::from(
                e.cloud_asr
                    .as_ref()
                    .and_then(|p| dito_core::provider::configured_asr::AsrProfiles::NAMES.get(p.selected).copied())
                    .unwrap_or_else(|| lang.tr(e.provider.kind.label())),
            ),
            Source::Local => SharedString::from(
                models
                    .catalog()
                    .iter()
                    .find(|m| m.id == e.local_model)
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| e.local_model.clone()),
            ),
        }
    }

    /// Role labels describe the engine's purpose, independently of its live status color.
    fn health_rows(&self, lang: Lang) -> Vec<HealthRow> {
        let cfg = self.st.cfg.get();
        let health = Health::compute(&cfg, Some(&*self.st.local));
        vec![
            HealthRow {
                role: lang.tr(Tr::ModelsDictation),
                name: Self::health_name(lang, &cfg.asr, &self.st.models),
                status: Some(health.asr),
            },
            HealthRow {
                role: lang.tr(Tr::ModelsPolish),
                name: if cfg.polish.enabled {
                    Self::health_name(lang, &cfg.polish.engine, &self.st.models)
                } else {
                    SharedString::from(lang.tr(Tr::ModelsPolishOff))
                },
                status: cfg.polish.enabled.then_some(health.polish),
            },
        ]
    }

    fn health_hovered(shell: &ui::ScopeHandle) -> bool {
        ui::is_hovered_in(shell, &ElementId::from("health-dot-hit"))
            || ui::is_hovered_in(shell, &ElementId::from("health-card-hit"))
    }

    /// Status chip in the models page's language: a 6 px state dot and a short label — no pill, no
    /// sentences; the color and the word carry the state.
    fn health_chip(p: Palette, lang: Lang, status: &EngineStatus) -> Div {
        let (dot, label) = match status {
            EngineStatus::Remote => (p.blue_text, lang.tr(Tr::ModelsConfigured)),
            EngineStatus::Running => (p.success, lang.tr(Tr::ModelsRunning)),
            EngineStatus::Loading => (p.primary, lang.tr(Tr::MenubarLoading)),
            EngineStatus::Failed(_) => (p.danger, lang.tr(Tr::ModelsFailed)),
            EngineStatus::Stopped | EngineStatus::NotInstalled => (p.muted, lang.tr(Tr::MenubarStopped)),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .flex_none()
            .child(div().size(px(6.)).rounded_full().bg(dot))
            .child(text(label, 12., FontWeight::MEDIUM, dot).line_height(px(16.)))
    }

    /// Fixed text geometry inside the growing surface: reveal text without scaling or reflow.
    fn health_card(p: Palette, lang: Lang, rows: &[HealthRow], dot_size: Pixels, width: f32) -> Div {
        let label_left = HEALTH_RADIUS - f32::from(dot_size) / 2.;
        let role_width = 58.;
        let name_left = label_left + role_width + 12.;
        let mut card = div().absolute().left_0().top_0().w(px(width)).h(px(HEALTH_HEIGHT)).child(
            div()
                .absolute()
                .left(px(45.))
                .top(px(HEALTH_PADDING))
                .h(px(HEALTH_ROW_HEIGHT))
                .flex()
                .items_center()
                .child(text(lang.tr(Tr::ModelsState), 14., FontWeight::BOLD, p.text).line_height(px(19.))),
        );
        for (index, row) in rows.iter().enumerate() {
            let top = HEALTH_PADDING + (index + 1) as f32 * (HEALTH_ROW_HEIGHT + HEALTH_ROW_GAP);
            card = card
                .child(
                    div()
                        .absolute()
                        .left(px(label_left))
                        .top(px(top))
                        .w(px(role_width))
                        .h(px(22.))
                        .rounded_full()
                        .bg(if p.dark { p.bg } else { p.tile_idle })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(text(row.role, 11., FontWeight::MEDIUM, p.muted).line_height(px(16.))),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(name_left))
                        .top(px(top))
                        .w(px(width - name_left - 20.))
                        .h(px(22.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.))
                        .child(
                            div().flex_1().min_w(px(0.)).child(
                                text(
                                    row.name.clone(),
                                    13.,
                                    FontWeight::MEDIUM,
                                    if row.status.is_some() { p.text } else { p.muted },
                                )
                                .line_height(px(19.))
                                .truncate(),
                            ),
                        )
                        .children(row.status.as_ref().map(|status| Self::health_chip(p, lang, status))),
                );
        }
        card
    }

    /// The glow is wider than the visible punctuation; the card adds a further 10 px outside it.
    fn health_glow(size: Pixels, color: Hsla, opacity: f32) -> Div {
        div()
            .absolute()
            .left(size / 2. - px(13.))
            .top(size / 2. - px(13.))
            .size(px(26.))
            .rounded_full()
            .bg(Hsla { a: 0.20 * opacity, ..color })
            .shadow(vec![BoxShadow {
                color: Hsla { a: 0.18 * opacity, ..color },
                offset: point(px(0.), px(0.)),
                blur_radius: px(6.),
                spread_radius: px(0.),
                inset: false,
            }])
    }

    /// Measure the real punctuation so the floating corner also follows font and rail changes.
    fn health_dot(&self, size: Pixels, color: Hsla) -> Div {
        let hit_id = ElementId::from("health-dot-hit");
        let hovered = Self::health_hovered(&self.motion.handle());
        let bounds = self.health_dot_bounds.clone();
        div()
            .relative()
            .flex_none()
            .size(size)
            .rounded_full()
            .bg(color)
            .child(
                canvas(
                    move |actual, window, _| {
                        if bounds.replace(Some(actual)) != Some(actual) {
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(ui::tweened(ElementId::from("health-halo"), if hovered { 1. } else { 0. }, 150, move |t| {
                Self::health_glow(size, color, t)
            }))
            .child(
                div()
                    .id(hit_id.clone())
                    .absolute()
                    .left(size / 2. - px(15.))
                    .top(size / 2. - px(15.))
                    .size(px(30.))
                    .rounded_full()
                    .on_hover(ui::hover_tracker(hit_id)),
            )
    }

    /// The dot is both logo punctuation and the center of the card's top-left corner.
    /// Prime the tween while closed, and keep it mounted to reverse smoothly on mouse-out.
    fn health_overlay(&self, c: &Ctx) -> AnyElement {
        let hovered = Self::health_hovered(&self.motion.handle());
        let rows = self.health_rows(c.lang);
        let p = c.p;
        let lang = c.lang;
        let color = if rows.iter().any(|row| row.status.as_ref().is_some_and(EngineStatus::is_problem)) {
            p.danger
        } else {
            p.success
        };
        let bounds = self.health_dot_bounds.get().unwrap_or_default();
        let dot_size = bounds.size.width;
        let width = 322.;
        let radius = px(HEALTH_RADIUS);
        let reduced = dito_platform::system::reduce_motion();
        let hit_id = ElementId::from("health-card-hit");
        div()
            .id(hit_id.clone())
            .absolute()
            .left(bounds.center().x - radius)
            .top(bounds.center().y - radius)
            // The stable hit surface includes both the dot and the card. It never replaces the
            // animated element's handler mid-flight, and cannot intercept anything while closed.
            .w(px(if hovered { width } else { 0. }))
            .h(px(if hovered { HEALTH_HEIGHT } else { 0. }))
            .rounded(radius)
            .on_hover(ui::hover_tracker(hit_id))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(ui::tweened(
                ElementId::from("health-card"),
                if hovered { 1. } else { 0. },
                if reduced { 120 } else { HEALTH_MS },
                move |t| {
                    if t <= 0.001 {
                        return div();
                    }
                    let progress = if reduced { 1. } else { t };
                    let text_opacity = if reduced { t } else { ((t - 0.25) / 0.75).clamp(0., 1.) };
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(46. + (width - 46.) * progress))
                        .h(px(46. + (HEALTH_HEIGHT - 46.) * progress))
                        .rounded(radius)
                        .bg(p.menu_bg)
                        .opacity(t)
                        .shadow(vec![BoxShadow {
                            color: Hsla { a: if p.dark { 0.28 } else { 0.12 }, ..p.shadow },
                            offset: point(px(0.), px(4.)),
                            blur_radius: px(18.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                        .child(div().absolute().inset_0().rounded(radius).border_1().border_color(p.border))
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .rounded(radius)
                                .overflow_hidden()
                                .child(Self::health_card(p, lang, &rows, dot_size, width).opacity(text_opacity)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(radius - dot_size / 2.)
                                .top(radius - dot_size / 2.)
                                .size(dot_size)
                                .child(Self::health_glow(dot_size, color, 1.))
                                .child(div().absolute().inset_0().rounded_full().bg(color)),
                        )
                },
            ))
            .into_any_element()
    }

    /// Collapsed logo: the app-icon mark — a 48 px D with the health dot tucked into the bowl's
    /// bottom-right. The wordmark itself never expands; the status card uses the measured dot bounds.
    fn logo_mark(&self, c: &Ctx) -> impl IntoElement {
        let p = &c.p;
        let rows = self.health_rows(c.lang);
        let healthy = rows.iter().all(|row| row.status.as_ref().is_none_or(|s| !s.is_problem()));
        let color = if healthy { p.success } else { p.danger };
        ui::drag_area("sidebar-brand-rail")
            .w_full()
            .h(px(51.))
            .relative()
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .top(px(-1.5))
                    .child(ui::text("D", 48., FontWeight::SEMIBOLD, p.text).line_height(relative(1.))),
            )
            .child(div().absolute().left(px(29.)).top(px(28.)).child(self.health_dot(px(13.5), color)))
    }

    /// The floating row reveal of the collapsed rail. One tween drives both the pill width (44 → 200)
    /// and the label fade, so background and text arrive together. An invisible hit pill keeps the
    /// hover (and forwards clicks) once the pointer travels past the rail edge onto the label.
    fn rail_hover(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let mut layer = div().absolute().size_full();
        for pg in Page::NAV {
            let nav_id = ElementId::from(("nav", pg as usize));
            let pill_id = ElementId::from(("rail-pill", pg as usize));
            let hovered = ui::is_hovered(&nav_id) || ui::is_hovered(&pill_id);
            let y = NAV_COL_Y + nav_y(pg);
            let pick = cx.listener(move |this, _, _, cx| this.set_page(pg, cx));
            if hovered {
                layer = layer.child(
                    div()
                        .id(pill_id.clone())
                        .absolute()
                        .left(px(RAIL_PADDING + 4.))
                        .top(px(y))
                        .w(px(200.))
                        .h(px(NAV_H))
                        .rounded(px(12.))
                        .cursor_pointer()
                        .on_hover(ui::hover_tracker(pill_id.clone()))
                        .on_click(pick),
                );
            }
            let ic = pg.icon();
            let label = SharedString::from(c.lang.tr(pg.label()));
            let tween_id = ElementId::from(("rail-reveal", pg as usize));
            // Clicking a row selects it: the reveal hides at once (no fade) and the sliding plate
            // alone carries the transition — two moving layers would read as out of step.
            let selected = self.page == pg;
            let target = if hovered && !selected { 1. } else { 0. };
            let ms = if selected { 0 } else { RAIL_MS };
            layer = layer.child(ui::tweened(tween_id, target, ms, move |hv| {
                if hv <= 0.001 {
                    return div().into_any_element();
                }
                // Fixed-size card that fades in as a whole: no width animation, so the label
                // never wraps mid-reveal and the icon never moves.
                div()
                    .absolute()
                    .left(px(RAIL_PADDING + 4.))
                    .top(px(y))
                    .w(px(200.))
                    .h(px(NAV_H))
                    .rounded(px(12.))
                    .bg(p.card)
                    .shadow(vec![BoxShadow {
                        color: p.shadow,
                        offset: point(px(0.), px(6.)),
                        blur_radius: px(24.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .opacity(hv)
                    .child(div().absolute().left(px(13.)).top(px(13.)).child(ui::icon(ic, 18., p.text)))
                    .child(div().absolute().left(px(43.)).top(px(11.)).child(text(
                        label.clone(),
                        14.,
                        FontWeight::MEDIUM,
                        p.text,
                    )))
                    .into_any_element()
            }));
        }
        layer.into_any_element()
    }

    fn sidebar(&self, c: &Ctx, cx: &mut Context<Self>, rail: bool) -> impl IntoElement {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let _ = &cfg;

        // Items are transparent hit areas (with the hover tint); icons and labels live in one layer above the
        // plate, so their brightness follows the plate as it slides under them.
        let nav = |page: Page, cx: &mut Context<Self>| {
            let on = self.page == page;
            let id = ElementId::from(("nav", page as usize));
            let hit = div()
                .id(id.clone())
                .h(px(NAV_H))
                .relative()
                .rounded(px(12.))
                .cursor_pointer()
                .on_hover(ui::hover_tracker(id.clone()))
                .on_click(cx.listener(move |this, _, _, cx| this.set_page(page, cx)))
                .child(ui::hover_fill_if(&id, Some(12.), ui::hex_alpha(p.nav_on, 0.7), !on));
            // Rail mode centers a square 44×44 pill (the expanded row plate is the same shape); the
            // wrapper keeps the full-width hit strip of the expanded sidebar.
            if rail {
                div().w_full().h(px(NAV_H)).flex().justify_center().child(hit.w(px(NAV_H))).into_any_element()
            } else {
                hit.w_full().into_any_element()
            }
        };
        let section = |label: Tr| {
            if rail {
                // Section headers reduce to a centered hairline; the 36 px slot stays so rows keep their y.
                ui::drag_area(label.id())
                    .w_full()
                    .h(px(NAV_SECTION_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(16.)).h(px(1.5)).rounded(px(0.75)).bg(p.hair))
                    .into_any_element()
            } else {
                ui::drag_area(label.id())
                    .w_full()
                    .h(px(NAV_SECTION_H))
                    .px(px(14.))
                    .flex()
                    .items_end()
                    .pb(px(6.))
                    .child(text(c.lang.tr(label), 11., FontWeight::MEDIUM, p.muted))
                    .into_any_element()
            }
        };
        let plate_y = nav_y(self.page);
        let shell_scope = ui::motion_scope();
        let (nav_on, text_c, muted_c, selected) = (p.nav_on, p.text, p.muted, self.page);
        let motion = ui::plate_state(ElementId::from("nav-plate"), (plate_y, plate_y + NAV_H));
        let plate = motion.animate("plate", move |top, bottom, energy| {
            let lit = ui::mix(nav_on, gpui_kit::hsla(0., 0., 1., 1.), 0.12 * energy);
            let plate = div().absolute().top(px(top)).h(px(bottom - top)).rounded(px(12.)).bg(lit);
            if rail {
                // Square pill over the icon, same left edge the hover reveal grows from. The nav
                // column already sits inside the rail's symmetric padding, so offsets are column-relative.
                plate.left(px(4.)).w(px(NAV_H))
            } else {
                plate.left(px(0.)).w_full()
            }
        });
        let entries: Vec<(Page, SharedString, bool)> = Page::NAV
            .iter()
            .map(|pg| {
                let hovered = ui::is_hovered(&ElementId::from(("nav", *pg as usize))) && *pg != selected;
                (*pg, SharedString::from(c.lang.tr(pg.label())), hovered)
            })
            .collect();
        let contents = motion.animate("content", move |top, bottom, _| {
            let mut layer = div().absolute().top(px(0.)).left(px(0.)).size_full();
            for (pg, label, hovered) in entries.iter() {
                let y = nav_y(*pg);
                let cover = ui::coverage(top, bottom, y, y + NAV_H);
                let weight = if cover >= 0.5 { FontWeight::MEDIUM } else { FontWeight::NORMAL };
                let (ic, label) = (pg.icon(), label.clone());
                let hover_id = ElementId::from(("nav-hover", *pg as usize));
                let row = ui::tweened_at(
                    shell_scope.clone(),
                    hover_id,
                    if *hovered { 0.55 } else { 0. },
                    ui::HOVER_MS,
                    move |hv| {
                        let color = ui::mix(muted_c, text_c, hv.max(cover));
                        let mut row = div().flex().items_center().child(ui::icon(ic, 18., color));
                        if !rail {
                            row = row.gap(px(12.)).child(text(label.clone(), 14., weight, color));
                        }
                        row
                    },
                );
                let slot = if rail {
                    // Icon centered on the rail's axis (column-relative); the label lives in the
                    // floating reveal instead.
                    div().absolute().left(px(17.)).top(px(y)).h(px(NAV_H)).flex().items_center()
                } else {
                    div().absolute().left(px(0.)).top(px(y)).w_full().h(px(NAV_H)).px(px(14.)).flex().items_center()
                };
                layer = layer.child(slot.child(row));
            }
            layer
        });

        let rows = self.health_rows(c.lang);
        let healthy = rows.iter().all(|row| row.status.as_ref().is_none_or(|s| !s.is_problem()));
        let dot_color = if healthy { p.success } else { p.danger };

        div()
            .w(px(if rail { RAIL_WIDTH } else { 232. }))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(if rail { RAIL_PADDING } else { 16. }))
            .pb(px(20.))
            .bg(p.side)
            .border_r_1()
            .border_color(p.border)
            // The whole sidebar background drags: the strip under the traffic lights, the logo row,
            // section headers and the empty stretch below the nav. Nav items keep their own clicks.
            .child(ui::drag_area("sidebar-top").h(px(sidebar_top_height(rail))).flex_none())
            .child(if rail {
                self.logo_mark(c).into_any_element()
            } else {
                ui::drag_area("sidebar-brand")
                    .px(px(12.))
                    .pb(px(14.))
                    .flex()
                    .items_end()
                    .child(
                        // The tracking after the last letter is the space before the dot: O-to-dot equals T-to-O.
                        ui::tracked("DITO", 34., FontWeight::SEMIBOLD, p.text, 0.14),
                    )
                    // mb keeps the dot's ink bottom on the wordmark's ink bottom (items_end
                    // alone would align to the line box).
                    .child(self.health_dot(px(12.), dot_color).mb(px(6.)))
                    .into_any_element()
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(NAV_GAP))
                    .child(plate)
                    .child(section(Tr::NavSettings))
                    .children([Page::General, Page::Models, Page::Dictionary, Page::Prompts].map(|pg| nav(pg, cx)))
                    .child(section(Tr::NavActivity))
                    .children([Page::History, Page::Stats, Page::Status].map(|pg| nav(pg, cx)))
                    .child(contents),
            )
            // The empty stretch under the nav drags the window, like a native sidebar background.
            .child(ui::drag_area("sidebar-drag").flex_1())
    }
}

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The shell (sidebar, health card, rail reveal) draws under the window's persistent scope:
        // its nav plate keeps moving across page switches, which reset only page scopes.
        let shell = self.motion.handle();
        ui::with_motion_scope(shell, || self.render_shell(_window, cx))
    }
}

impl Settings {
    fn render_shell(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Auto-collapse with hysteresis: the window crossing RAIL_COLLAPSE folds the sidebar into the
        // icon rail; it only unfolds once the window is past RAIL_EXPAND.
        let vw = f32::from(_window.viewport_size().width);
        let rail = if self.rail { vw < RAIL_EXPAND } else { vw < RAIL_COLLAPSE };
        if rail != self.rail {
            self.rail = rail;
            _window.set_traffic_light_position(traffic_light_position(rail));
            cx.notify();
        }
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        theme::apply(&p, cx);
        let page_view: AnyElement = match self.page {
            Page::General => self.general.clone().into_any_element(),
            Page::Models => self.models.clone().into_any_element(),
            Page::Dictionary => self.dictionary.clone().into_any_element(),
            Page::Prompts => self.prompts.clone().into_any_element(),
            Page::History => self.history.clone().into_any_element(),
            Page::Stats => self.stats.clone().into_any_element(),
            Page::Status => self.status.clone().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(ui::cjk_family())
            // Any click first drops keyboard focus; an input under the pointer then takes it back.
            .capture_any_mouse_down(|_, window, cx| window.blur(cx))
            .child(self.sidebar(&c, cx, rail))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .relative()
                    .overflow_hidden()
                    .child(ui::prepare_card_backdrop(self.page.english(), p))
                    .child(ui::page_wordmark(self.page.english(), p))
                    .child(page_view)
                    // Edge gutters over the page's horizontal padding also drag. Scroll still
                    // passes through, and content never enters these zones, so nothing is blocked.
                    .child(
                        ui::drag_area("gutter-left")
                            .absolute()
                            .top_0()
                            .left_0()
                            .h_full()
                            .w(px(56.))
                            .block_mouse_except_scroll(),
                    )
                    .child(
                        ui::drag_area("gutter-right")
                            .absolute()
                            .top_0()
                            .right_0()
                            .h_full()
                            .w(px(56.))
                            .block_mouse_except_scroll(),
                    ),
            )
            // Last child: the rail's floating reveal paints above the page content.
            .when(rail, |el| el.child(self.rail_hover(&c, cx)))
            // Painted last so the health card floats above the page content, not under it.
            .child(self.health_overlay(&c))
            .children(if self.page == Page::Models { self.models.read(cx).editor_view() } else { None })
    }
}

/// Standard page shell: scrolling column, title row with optional actions, then the body.
/// Like [`page`] but without the page-level scroll: for bodies that manage their own scrolling
/// (virtualized lists) and need the remaining height.
pub fn page_fill(
    p: Palette,
    id: &'static str,
    title: SharedString,
    actions: Option<AnyElement>,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id)
        .size_full()
        .px(px(56.))
        .pt(px(44.))
        .pb(px(36.))
        .flex()
        .flex_col()
        .gap(px(28.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.))
                .child(ui::page_title(&p, title))
                .children(actions),
        )
        .child(body)
}

pub fn page(
    p: Palette,
    id: &'static str,
    title: SharedString,
    actions: Option<AnyElement>,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id)
        .size_full()
        .overflow_y_scroll()
        .px(px(56.))
        .pt(px(44.))
        .pb(px(36.))
        .flex()
        .flex_col()
        .gap(px(28.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.))
                .child(ui::page_title(&p, title))
                .children(actions),
        )
        .child(body)
}

struct SettingsHandle {
    window: WindowHandle<gpui_kit::component::Root>,
    view: Entity<Settings>,
}

impl Global for SettingsHandle {}

/// Keeps the settings window-close observer alive independently of the handle it watches: the
/// callback removes that handle, so storing the subscription inside it would unsubscribe itself
/// mid-callback. Replaced (and so dropped, safely outside any callback) by the next settings window.
struct SettingsCloseWatch {
    _subscription: gpui_kit::Subscription,
}

impl Global for SettingsCloseWatch {}

#[cfg(feature = "screenshots")]
pub fn handle(cx: &App) -> Option<(WindowHandle<gpui_kit::component::Root>, Entity<Settings>)> {
    cx.try_global::<SettingsHandle>().map(|h| (h.window, h.view.clone()))
}

/// The settings shell's motion scope, for the screenshot harness.
#[cfg(feature = "screenshots")]
pub fn shell_motion(cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<SettingsHandle>().map(|h| h.view.read(cx).motion.handle())
}

/// One page's motion scope, for the screenshot harness.
#[cfg(feature = "screenshots")]
pub fn page_motion(page: Page, cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<SettingsHandle>().map(|h| {
        let view = h.view.read(cx);
        match page {
            Page::General => view.general.read(cx).motion_scope(),
            Page::Models => view.models.read(cx).motion_scope(),
            Page::Dictionary => view.dictionary.read(cx).motion_scope(),
            Page::Prompts => view.prompts.read(cx).motion_scope(),
            Page::History => view.history.read(cx).motion_scope(),
            Page::Stats => view.stats.read(cx).motion_scope(),
            Page::Status => view.status.read(cx).motion_scope(),
        }
    })
}

/// Open (or focus) the settings window on `page`.
pub fn open(st: Arc<AppState>, page: Page, cx: &mut App) {
    // Reuse an existing window.
    if let Some(h) = cx.try_global::<SettingsHandle>() {
        let (window, view) = (h.window, h.view.clone());
        // Navigating from inside the settings window (a button click): that window is mid-update
        // and gpui rejects nested window updates, so update the view directly — notify re-renders
        // the window, and a clicked window is by definition already active. Doing window.update
        // here would report "window not found" and open a duplicate window.
        if cx.active_window().is_some_and(|active| active.window_id() == window.window_id()) {
            view.update(cx, |s, cx| s.set_page(page, cx));
            cx.activate(true);
            return;
        }
        if window
            .update(cx, |_, window, cx| {
                view.update(cx, |s, cx| s.set_page(page, cx));
                window.activate_window();
            })
            .is_ok()
        {
            cx.activate(true);
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(1120.), px(860.)), cx);
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Dito".into()),
            appears_transparent: true,
            traffic_light_position: Some(traffic_light_position(false)),
        }),
        window_min_size: Some(size(px(900.), px(640.))),
        ..Default::default()
    };
    let mut view_slot = None;
    let res = cx.open_window(opts, |window, cx| {
        let view = cx.new(|cx| Settings::new(st, page, window, cx));
        view_slot = Some(view.clone());
        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
    });
    match (res, view_slot) {
        (Ok(window), Some(view)) => {
            cx.set_global(SettingsHandle { window, view });
            // The global holds the view strongly, so closing the window alone would not release
            // it. Watch for exactly this window's close: stop the page's data lifecycle,
            // invalidate every shell and page scope, and drop the handle. Both ids must match —
            // the closed window and the window of the handle currently installed — so neither a
            // close of any other window nor an old observer can touch a newer window's handle.
            let id = window.window_id();
            let close = cx.on_window_closed(move |cx, closed| {
                if closed != id {
                    return;
                }
                let Some(h) = cx.try_global::<SettingsHandle>() else { return };
                if h.window.window_id() != id {
                    return;
                }
                let view = h.view.clone();
                cx.remove_global::<SettingsHandle>();
                view.update(cx, |s, cx| s.on_window_closed(cx));
            });
            cx.set_global(SettingsCloseWatch { _subscription: close });
        }
        (Err(e), _) => tracing::error!("could not open settings: {e}"),
        _ => {}
    }
    cx.activate(true);
}

pub fn global_state(cx: &App) -> Arc<AppState> {
    cx.global::<AppGlobal>().0.clone()
}
