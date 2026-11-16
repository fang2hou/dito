//! The local API tool: the loopback service and its MCP endpoint (`design:tools/local-api`, `design:tools/mcp`).
//!
//! The page only writes settings: the service follows them (see `dito_core::local_api`).

mod dialogs;
mod spec;
mod view;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::local_api::{CheckFailure, Surface, check_connection};
use gpui_kit::prelude::*;
use gpui_kit::*;

use self::spec::{Item, Tab};
use crate::services;
use crate::settings::Ctx;
use crate::state::AppState;
use crate::ui::{self, InputEvent, InputState};

/// How long the connection check keeps showing its answer.
const CHECK_RESULT_MS: u64 = 3000;
/// How long the copy specification button keeps its check.
const SPEC_COPIED_MS: u64 = 1500;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialog {
    Settings,
    Reference(Item),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    Idle,
    Running,
    Done(Result<(), CheckFailure>),
}

pub(crate) struct LocalApiPage {
    st: Arc<AppState>,
    motion: ui::MotionScope,
    tab: Tab,
    dialog: Option<Dialog>,
    focus: FocusHandle,
    port: Entity<InputState>,
    token: Entity<InputState>,
    port_invalid: bool,
    check: Check,
    check_epoch: u64,
    spec_copied: Option<Instant>,
}

impl LocalApiPage {
    pub(crate) fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        ui::notify_on_change(st.local_api.status(), cx);
        let port_value = st.cfg.get().local_api.port.to_string();
        let port = cx.new(|cx| InputState::new(window, cx).default_value(port_value));
        ui::notify_on_focus(&port, window, cx);
        cx.subscribe_in(&port, window, |page: &mut Self, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                page.save_port(state.read(cx).value().as_ref(), cx);
            }
        })
        .detach();
        let token_value = st.local_api.token();
        let token = cx.new(|cx| InputState::new(window, cx).default_value(token_value));
        ui::notify_on_focus(&token, window, cx);
        cx.subscribe_in(&token, window, |page: &mut Self, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                let value = state.read(cx).value().to_string();
                if let Err(e) = page.st.local_api.set_token(&value) {
                    tracing::warn!("could not save the Local API token: {e}");
                }
                cx.notify();
            }
        })
        .detach();
        Self {
            st,
            motion: ui::MotionScope::new(),
            tab: Tab::Api,
            dialog: None,
            focus: cx.focus_handle(),
            port,
            token,
            port_invalid: false,
            check: Check::Idle,
            check_epoch: 0,
            spec_copied: None,
        }
    }

    /// The page became the visible one, or stopped being it.
    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if !active {
            self.dialog = None;
            self.check = Check::Idle;
            self.motion.reset();
        }
        cx.notify();
    }

    fn ctx(&self, cx: &App) -> Ctx {
        Ctx::new(&self.st, cx)
    }

    /// Only a port a program may listen on is saved; anything else keeps the field marked and the service as it was.
    fn save_port(&mut self, value: &str, cx: &mut Context<Self>) {
        match value.trim().parse::<u16>() {
            Ok(port) if port >= 1024 => {
                self.port_invalid = false;
                services::save_config(&self.st, move |cfg| cfg.local_api.port = port);
            }
            _ => self.port_invalid = true,
        }
        cx.notify();
    }

    fn open_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Some(dialog);
        ui::dismiss_tips(&self.motion.handle());
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn close_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        cx.notify();
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.check = Check::Idle;
        cx.notify();
    }

    fn run_check(&mut self, cx: &mut Context<Self>) {
        if self.check == Check::Running {
            return;
        }
        self.check = Check::Running;
        self.check_epoch += 1;
        let epoch = self.check_epoch;
        let (port, token) = (self.st.cfg.get().local_api.port, self.st.local_api.token());
        let surface = if self.tab == Tab::Api { Surface::Api } else { Surface::Mcp };
        let rt = self.st.rt.clone();
        cx.spawn(async move |this, cx| {
            let answer = rt.spawn(async move { check_connection(port, &token, surface).await }).await;
            let answer = answer.unwrap_or(Err(CheckFailure::Unreachable));
            let _ = this.update(cx, |page, cx| {
                if page.check_epoch == epoch {
                    page.check = Check::Done(answer);
                    cx.notify();
                }
            });
            cx.background_executor().timer(Duration::from_millis(CHECK_RESULT_MS)).await;
            let _ = this.update(cx, |page, cx| {
                if page.check_epoch == epoch {
                    page.check = Check::Idle;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Puts the Markdown on the clipboard and shows the check on the button for a moment.
    fn copy_spec(&mut self, cx: &mut Context<Self>) {
        let c = self.ctx(cx);
        let facts = self.facts();
        let t = |tr| c.t(tr).to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(spec::tab_markdown(self.tab, &facts, &t)));
        self.spec_copied = Some(Instant::now());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(SPEC_COPIED_MS)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        cx.notify();
    }

    fn facts(&self) -> spec::Facts {
        let cfg = self.st.cfg.get();
        let [text, speech] = dito_core::local_api::served_models(&cfg);
        spec::Facts {
            base_url: format!("http://127.0.0.1:{}", cfg.local_api.port),
            text_model: text.id,
            speech_model: speech.id,
            api_any_token: cfg.local_api.api_any_token,
            mcp_any_token: cfg.local_api.mcp_any_token,
        }
    }
}

impl Render for LocalApiPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::render_with_scope(self.motion.handle(), || self.view(window, cx))
    }
}

/// The states of the design boards, for the screenshot harness.
#[cfg(feature = "screenshots")]
#[derive(Clone, Copy)]
pub(crate) enum LocalApiPose {
    Api,
    Mcp,
    Settings,
    /// The reference of the nth item, the API items first.
    Reference(usize),
}

#[cfg(feature = "screenshots")]
impl LocalApiPage {
    pub(crate) fn pose(&mut self, pose: LocalApiPose, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        match pose {
            LocalApiPose::Api => self.tab = Tab::Api,
            LocalApiPose::Mcp => self.tab = Tab::Mcp,
            LocalApiPose::Settings => self.open_dialog(Dialog::Settings, window, cx),
            LocalApiPose::Reference(n) => {
                let items: Vec<Item> = Item::API.into_iter().chain(Item::MCP).collect();
                self.tab = if n < 3 { Tab::Api } else { Tab::Mcp };
                self.open_dialog(Dialog::Reference(items[n]), window, cx);
            }
        }
        cx.notify();
    }
}
