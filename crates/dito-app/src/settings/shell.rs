use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::models;
use super::{Page, general, history, prompts, stats, status, wordbook};
use crate::app_shell::{self, Chrome, NavHost, ShellView, WindowDestination};
use crate::services;
use crate::state::AppState;
use crate::ui;

pub struct Settings {
    pub(super) chrome: Chrome,
    page: Page,
    general: Entity<general::GeneralPage>,
    models: Entity<models::ModelsPage>,
    wordbook: Entity<wordbook::WordbookPage>,
    prompts: Entity<prompts::PromptsPage>,
    history: Entity<history::HistoryPage>,
    stats: Entity<stats::StatsPage>,
    status: Entity<status::StatusPage>,
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
            chrome: Chrome::new(st),
            page,
        };
        // The modal is painted by the window shell, above the sidebar as well as the page.
        cx.subscribe(&this.models, |_, _, _: &models::ModalChanged, cx| cx.notify()).detach();
        cx.subscribe(&this.wordbook, |_, _, _: &wordbook::ModalChanged, cx| cx.notify()).detach();
        cx.observe(&this.prompts, |_, _, cx| cx.notify()).detach();
        this.activate(page, cx);
        this
    }

    fn activate(&self, page: Page, cx: &mut Context<Self>) {
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

    /// Shows the Models page with the text model tab selected.
    pub fn show_text(&mut self, cx: &mut Context<Self>) {
        self.set_page(Page::Models, cx);
        self.models.update(cx, |models, cx| models.show_text(cx));
    }

    /// Shows the Models page with the speech-detection tab selected.
    pub fn show_vad(&mut self, cx: &mut Context<Self>) {
        self.set_page(Page::Models, cx);
        self.models.update(cx, |models, cx| models.show_vad(cx));
    }

    fn reset_page_motion(&self, page: Page, cx: &mut Context<Self>) {
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

    #[cfg(feature = "screenshots")]
    pub(super) fn page_motion(&self, page: Page, cx: &App) -> ui::ScopeHandle {
        match page {
            Page::General => self.general.read(cx).motion_scope(),
            Page::Models => self.models.read(cx).motion_scope(),
            Page::Wordbook => self.wordbook.read(cx).motion_scope(),
            Page::Prompts => self.prompts.read(cx).motion_scope(),
            Page::History => self.history.read(cx).motion_scope(),
            Page::Stats => self.stats.read(cx).motion_scope(),
            Page::Status => self.status.read(cx).motion_scope(),
        }
    }
}

impl ShellView for Settings {
    /// Invalidates every transient scope (shell and pages), so handles still held elsewhere (tests, screenshots) read
    /// static defaults instead of stale motion state.
    fn on_window_closed(&mut self, cx: &mut Context<Self>) {
        self.deactivate(self.page, cx);
        self.chrome.reset_motion();
        for page in Page::ALL {
            self.reset_page_motion(page, cx);
        }
    }
}

impl NavHost for Settings {
    type Page = Page;

    fn chrome(&self) -> &Chrome {
        &self.chrome
    }

    fn nav_page(&self) -> Page {
        self.page
    }

    fn switch_destination(&self) -> WindowDestination {
        WindowDestination::Tools
    }

    fn select_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.set_page(page, cx);
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The shell (sidebar, health card, rail reveal) draws under the window's persistent scope:
        // its nav plate keeps moving across page switches, which reset only page scopes.
        let shell = self.chrome.motion().owned_by(cx.entity_id());
        ui::render_with_scope(shell, || ui::tooltip_root(self.render_shell(window, cx)))
    }
}

impl Settings {
    fn page_view(&self, window: &Window) -> AnyElement {
        match self.page {
            Page::General => self.general.clone().into_any_element(),
            Page::Models => self.models.clone().into_any_element(),
            Page::Wordbook => self.wordbook.clone().into_any_element(),
            Page::Prompts => self.prompts.clone().into_any_element(),
            Page::History => self.history_view(window),
            Page::Stats => self.stats.clone().into_any_element(),
            Page::Status => self.status.clone().into_any_element(),
        }
    }

    /// Reused from the previous frame until the page is notified, so a frame that only the sidebar asked for does not
    /// build the rows again. A reused frame skips the page's elements, and those are what report accessibility nodes,
    /// so the page is drawn every frame while an assistive client is connected.
    fn history_view(&self, window: &Window) -> AnyElement {
        if window.is_a11y_active() {
            return self.history.clone().into_any_element();
        }
        self.history.clone().cached(StyleRefinement::default().size_full()).into_any_element()
    }

    fn render_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = self.chrome.begin_frame(window, cx);
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
        app_shell::frame(self, &c, Some(self.page.english()), self.page_view(window), window, cx)
            .children(prompt_overlay)
            .children(models_overlay)
            .children(wordbook_overlay)
    }
}
