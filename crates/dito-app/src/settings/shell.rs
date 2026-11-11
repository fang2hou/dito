use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::models;
use super::page::{RAIL_COLLAPSE, RAIL_EXPAND, traffic_light_position};
use super::{Ctx, Page, general, history, prompts, stats, status, wordbook};
use crate::services;
use crate::state::AppState;
use crate::theme::{self, Palette};
use crate::ui;

pub struct Settings {
    pub(super) st: Arc<AppState>,
    pub(super) page: Page,
    pub(super) rail: bool,
    pub(super) health_dot_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The persistent shell scope: the sidebar's nav plate and hover live here for the window's
    /// whole life; page navigation resets page scopes but never this one.
    pub(super) motion: ui::MotionScope,
    pub(super) general: Entity<general::GeneralPage>,
    pub(super) models: Entity<models::ModelsPage>,
    pub(super) wordbook: Entity<wordbook::WordbookPage>,
    pub(super) prompts: Entity<prompts::PromptsPage>,
    pub(super) history: Entity<history::HistoryPage>,
    pub(super) stats: Entity<stats::StatsPage>,
    pub(super) status: Entity<status::StatusPage>,
}

#[cfg(feature = "screenshots")]
mod probe;

impl Settings {
    pub fn new(st: Arc<AppState>, page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let s = &st;
        let this = Self {
            general: cx.new(|cx| general::GeneralPage::new(s.clone(), cx)),
            models: cx.new(|cx| models::ModelsPage::new(s.clone(), window, cx)),
            wordbook: cx.new(|cx| wordbook::WordbookPage::new(s.clone(), window, cx)),
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
        cx.subscribe(&this.wordbook, |_, _, _: &wordbook::ModalChanged, cx| cx.notify()).detach();
        cx.observe(&this.prompts, |_, _, cx| cx.notify()).detach();
        this.activate(page, cx);
        this
    }

    pub(super) fn activate(&self, page: Page, cx: &mut Context<Self>) {
        match page {
            Page::History => self.history.update(cx, |h, cx| {
                // Heavy list: skeleton it while the nav plate settles so its frames stay smooth.
                h.enter(cx);
                h.set_active(true, cx);
            }),
            Page::Stats => self.stats.update(cx, |s, cx| s.set_active(true, cx)),
            Page::Wordbook => self.wordbook.update(cx, |d, cx| d.set_active(true, cx)),
            Page::Status => self.status.update(cx, |s, cx| s.set_active(true, cx)),
            _ => {}
        }
    }

    fn deactivate(&self, page: Page, cx: &mut Context<Self>) {
        match page {
            Page::History => self.history.update(cx, |h, cx| h.set_active(false, cx)),
            Page::Stats => self.stats.update(cx, |s, cx| s.set_active(false, cx)),
            Page::Wordbook => self.wordbook.update(cx, |d, cx| d.set_active(false, cx)),
            Page::Status => self.status.update(cx, |s, cx| s.set_active(false, cx)),
            _ => {}
        }
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
        self.activate(page, cx);
        cx.notify();
    }

    pub(super) fn reset_page_motion(&self, page: Page, cx: &mut Context<Self>) {
        match page {
            Page::General => self.general.update(cx, |g, _| g.reset_motion()),
            Page::Models => self.models.update(cx, |m, cx| m.reset_motion(cx)),
            Page::Wordbook => self.wordbook.update(cx, |d, _| d.reset_motion()),
            Page::Prompts => self.prompts.update(cx, |p, cx| p.reset_motion(cx)),
            Page::History => self.history.update(cx, |h, _| h.reset_motion()),
            Page::Stats => self.stats.update(cx, |s, _| s.reset_motion()),
            Page::Status => self.status.update(cx, |s, _| s.reset_motion()),
        }
    }

    /// Invalidates every transient scope (shell and pages), so handles still held elsewhere (tests, screenshots) read
    /// static defaults instead of stale motion state.
    pub(super) fn on_window_closed(&mut self, cx: &mut Context<Self>) {
        self.deactivate(self.page, cx);
        self.motion.reset();
        for page in Page::ALL {
            self.reset_page_motion(page, cx);
        }
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The shell (sidebar, health card, rail reveal) draws under the window's persistent scope:
        // its nav plate keeps moving across page switches, which reset only page scopes.
        let shell = self.motion.handle();
        ui::with_motion_scope(shell, || ui::tooltip_root(self.render_shell(window, cx)))
    }
}

impl Settings {
    /// Hysteresis: the sidebar folds at RAIL_COLLAPSE and only unfolds past RAIL_EXPAND. Returns whether the rail is
    /// showing.
    fn follow_window_width(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let vw = f32::from(window.viewport_size().width);
        let rail = if self.rail { vw < RAIL_EXPAND } else { vw < RAIL_COLLAPSE };
        if rail != self.rail {
            self.rail = rail;
            window.set_traffic_light_position(traffic_light_position(rail));
            cx.notify();
        }
        rail
    }

    fn page_view(&self) -> AnyElement {
        match self.page {
            Page::General => self.general.clone().into_any_element(),
            Page::Models => self.models.clone().into_any_element(),
            Page::Wordbook => self.wordbook.clone().into_any_element(),
            Page::Prompts => self.prompts.clone().into_any_element(),
            Page::History => self.history.clone().into_any_element(),
            Page::Stats => self.stats.clone().into_any_element(),
            Page::Status => self.status.clone().into_any_element(),
        }
    }

    /// Scroll still passes through, and content never enters these zones, so nothing is blocked.
    fn content(&self, p: Palette) -> Div {
        div()
            .flex_1()
            .h_full()
            .relative()
            .overflow_hidden()
            .child(ui::prepare_card_backdrop(self.page.english(), p))
            .child(ui::page_wordmark(self.page.english(), p))
            .child(self.page_view())
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
            )
    }

    fn render_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let rail = self.follow_window_width(window, cx);
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        theme::apply(&p, cx);
        let prompt_overlay =
            if self.page == Page::Prompts { self.prompts.update(cx, |p, cx| p.overlay(window, cx)) } else { None };
        let models_overlay = if self.page == Page::Models {
            self.models.update(cx, |models, cx| models.overlay(window, cx))
        } else {
            None
        };
        let wordbook_overlay = if self.page == Page::Wordbook {
            self.wordbook.update(cx, |wordbook, cx| wordbook.overlay(window, cx))
        } else {
            None
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
            .child(self.content(p))
            // Last child: the rail's floating reveal paints above the page content.
            .when(rail, |el| el.child(self.rail_hover(&c, cx)))
            // Painted last so the health card floats above the page content, not under it.
            .child(self.health_overlay(&c))
            .children(prompt_overlay)
            .children(models_overlay)
            .children(wordbook_overlay)
    }
}
