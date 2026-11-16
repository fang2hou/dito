//! The Tools window's shell: the shared sidebar with the meeting tools, and the content column the
//! transcription page renders into.

use std::sync::Arc;

use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::chat::ChatPage;
use super::local_api::LocalApiPage;
use super::transcription::TranscriptionPage;
use crate::app_shell::{self, Chrome, NavHost, NavPage, WindowDestination};
use crate::assets::Ic;
use crate::services;
use crate::settings::Ctx;
use crate::state::AppState;
use crate::theme::{self, Palette};
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ToolsPage {
    Transcription,
    Chat,
    LocalApi,
}

impl ToolsPage {
    pub(crate) fn english(self) -> &'static str {
        match self {
            Self::Transcription => "Transcription",
            Self::Chat => "Chat",
            Self::LocalApi => "Local API",
        }
    }
}

impl NavPage for ToolsPage {
    fn label(self) -> Tr {
        match self {
            Self::Transcription => Tr::NavTranscription,
            Self::Chat => Tr::NavChat,
            Self::LocalApi => Tr::NavLocalApi,
        }
    }

    fn icon(self) -> Ic {
        match self {
            Self::Transcription => Ic::AudioLines,
            Self::Chat => Ic::MessageCircleMore,
            Self::LocalApi => Ic::Cable,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn sections() -> &'static [(Tr, &'static [ToolsPage])] {
        &SECTIONS
    }
}

static SECTIONS: [(Tr, &[ToolsPage]); 1] =
    [(Tr::NavTools, &[ToolsPage::Transcription, ToolsPage::Chat, ToolsPage::LocalApi])];

pub struct Tools {
    chrome: Chrome,
    page: ToolsPage,
    transcription: Entity<TranscriptionPage>,
    chat: Entity<ChatPage>,
    local_api: Entity<LocalApiPage>,
    /// The session state the sidebar dot shows; cached so the row renders without reading the page.
    recording: bool,
    /// A paused session keeps its dot, in muted instead of danger.
    paused: bool,
}

#[cfg(feature = "screenshots")]
mod probe;

impl Tools {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let transcription = cx.new(|cx| TranscriptionPage::new(st.clone(), window, cx));
        transcription.update(cx, |page, cx| page.set_active(true, cx));
        let (recording, paused) = (transcription.read(cx).is_recording(), transcription.read(cx).is_paused());
        // The page redraws itself; the shell only needs to re-render when the recording dot appears, goes away or
        // turns muted for a pause.
        cx.observe(&transcription, |this, page, cx| {
            let (recording, paused) = (page.read(cx).is_recording(), page.read(cx).is_paused());
            if (recording, paused) != (this.recording, this.paused) {
                (this.recording, this.paused) = (recording, paused);
                cx.notify();
            }
        })
        .detach();
        let chat = cx.new(|cx| ChatPage::new(st.clone(), window, cx));
        let local_api = cx.new(|cx| LocalApiPage::new(st.clone(), window, cx));
        Self {
            chrome: Chrome::new(st),
            page: ToolsPage::Transcription,
            transcription,
            chat,
            local_api,
            recording,
            paused,
        }
    }

    #[cfg(feature = "screenshots")]
    pub(crate) fn transcription(&self) -> &Entity<TranscriptionPage> {
        &self.transcription
    }

    /// The window is being opened or brought forward: read the session's latest list into the page.
    pub(super) fn activate(&self, cx: &mut Context<Self>) {
        self.set_pages_active(Some(self.page), cx);
        cx.notify();
    }

    fn set_pages_active(&self, shown: Option<ToolsPage>, cx: &mut Context<Self>) {
        self.transcription.update(cx, |page, cx| page.set_active(shown == Some(ToolsPage::Transcription), cx));
        self.chat.update(cx, |page, cx| page.set_active(shown == Some(ToolsPage::Chat), cx));
        self.local_api.update(cx, |page, cx| page.set_active(shown == Some(ToolsPage::LocalApi), cx));
    }

    /// The window is closing: nothing transient may replay if it is opened again.
    pub(super) fn on_window_closed(&mut self, cx: &mut Context<Self>) {
        self.set_pages_active(None, cx);
        self.transcription.update(cx, |page, _| page.reset_motion());
        self.chrome.reset_motion();
    }
}

impl NavHost for Tools {
    type Page = ToolsPage;

    fn chrome(&self) -> &Chrome {
        &self.chrome
    }

    fn nav_page(&self) -> ToolsPage {
        self.page
    }

    fn switch_destination(&self) -> WindowDestination {
        WindowDestination::Settings
    }

    fn select_page(&mut self, page: ToolsPage, cx: &mut Context<Self>) {
        if self.page != page {
            self.page = page;
            self.set_pages_active(Some(page), cx);
        }
        cx.notify();
    }

    fn nav_badge(&self, page: ToolsPage, p: &Palette) -> Option<Hsla> {
        // A live session reads from the sidebar even while another tool is shown.
        (page == ToolsPage::Transcription && self.recording).then_some(if self.paused { p.muted } else { p.danger })
    }
}

impl Render for Tools {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shell = self.chrome.motion();
        ui::render_with_scope(shell, || ui::tooltip_root(self.render_shell(window, cx)))
    }
}

impl Tools {
    /// Scroll still passes through, and content never enters these zones, so nothing is blocked.
    fn content(&self, p: Palette) -> Div {
        let page = self.page;
        let view: AnyView = match page {
            ToolsPage::Transcription => self.transcription.clone().into(),
            ToolsPage::Chat => self.chat.clone().into(),
            ToolsPage::LocalApi => self.local_api.clone().into(),
        };
        div()
            .flex_1()
            .h_full()
            .relative()
            .overflow_hidden()
            .child(ui::prepare_card_backdrop(page.english(), p))
            .child(ui::page_wordmark(page.english(), p))
            .child(view)
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
        let changed = self.chrome.follow_window_width(window);
        let rail = self.chrome.rail();
        if changed {
            cx.notify();
        }
        let c = Ctx::new(self.chrome.st(), cx);
        let p = c.p;
        theme::apply(&p, cx);
        div()
            .size_full()
            .flex()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(ui::cjk_family())
            // Any click first drops keyboard focus; an input under the pointer then takes it back.
            .capture_any_mouse_down(|_, window, cx| window.blur(cx))
            .child(app_shell::sidebar(self, &c, cx, rail))
            .child(self.content(p))
            // Last child: the rail's floating reveal paints above the page content.
            .when(rail, |el| el.child(app_shell::rail_hover(self, &c, cx)))
            // Painted last so the health card floats above the page content, not under it.
            .child(app_shell::health_overlay(&self.chrome, &c, window))
    }
}
