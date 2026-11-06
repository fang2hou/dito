use std::sync::Arc;
use std::time::Duration;

use dito_audio::InputDevice;
use dito_core::controller::HudState;
use dito_core::i18n::Tr;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::*;

use crate::hotkeys::CaptureState;
use crate::hud::StylePreview;
use crate::services;
use crate::settings::hud_slider::HudHeightSlider;
use crate::settings::{self, Ctx};
use crate::state::AppState;
use crate::ui::{self, Cb, cbk};

const MIC_POLL: Duration = Duration::from_secs(1);

pub struct GeneralPage {
    pub(super) st: Arc<AppState>,
    pub(super) open: Option<&'static str>,
    /// Filled when the microphone list is opened; enumerating devices at launch touches the audio
    /// system (and any nearby iPhone) before the user has asked for a microphone.
    pub(super) devices: Vec<InputDevice>,
    /// Bumped each time the microphone list opens; ends the previous refresh loop.
    pub(super) mic_poll: ui::Requests,
    pub(super) capturing: bool,
    pub(super) accessibility_ok: bool,
    pub(super) hud_preview: Option<Entity<StylePreview>>,
    pub(super) hud_height: Entity<HudHeightSlider>,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    pub(super) motion: ui::MotionScope,
}

impl GeneralPage {
    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);

        ui::observe(st.hotkeys.capture.subscribe(), cx, |t, capture, cx| {
            t.capturing = capture == CaptureState::Waiting;
            cx.notify();
        });

        ui::notify_on_change(st.hotkeys.unavailable.subscribe(), cx);

        // Permissions change outside the app; poll lightly while the window exists.
        cx.spawn(async move |this, cx| {
            loop {
                let ok = permissions::status(Permission::Accessibility) == Status::Granted;
                if this
                    .update(cx, |t, cx| {
                        if t.accessibility_ok != ok {
                            t.accessibility_ok = ok;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(2)).await;
            }
        })
        .detach();

        ui::observe(st.dictation.hud(), cx, |page, state, cx| {
            if state != HudState::Hidden {
                page.cancel_hud_preview(cx);
            }
        });

        let hud_height = cx.new(|cx| HudHeightSlider::new(st.clone(), cx));
        Self {
            motion: ui::MotionScope::new(),
            hud_preview: None,
            hud_height,
            devices: Vec::new(),
            mic_poll: ui::Requests::default(),
            st,
            open: None,
            capturing: false,
            accessibility_ok: permissions::status(Permission::Accessibility) == Status::Granted,
        }
    }

    pub(super) fn toggle(&self, cx: &mut Context<Self>, id: &'static str) -> Cb {
        cbk(cx, move |this: &mut Self, _, cx| {
            this.open = if this.open == Some(id) { None } else { Some(id) };
            if this.open == Some("mic") && (!this.st.opts.mock || this.st.opts.real_mic) {
                this.devices = dito_audio::list_input_devices();
                let poll = this.mic_poll.begin();
                // Headsets connect a few seconds after they are put on: keep the list current while it is shown.
                cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(MIC_POLL).await;
                        let devices = cx.background_executor().spawn(async { dito_audio::list_input_devices() }).await;
                        let keep = this.update(cx, |t, cx| {
                            if t.open != Some("mic") || !t.mic_poll.is_current(poll) {
                                return false;
                            }
                            if t.devices != devices {
                                t.devices = devices;
                                cx.notify();
                            }
                            true
                        });
                        if !keep.unwrap_or(false) {
                            break;
                        }
                    }
                })
                .detach();
            }
            cx.notify();
        })
    }
}

impl GeneralPage {
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(in crate::settings) fn reset_motion(&mut self) {
        self.motion.reset();
    }
}

impl Render for GeneralPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(cx))
    }
}

impl GeneralPage {
    fn view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        settings::page(
            p,
            "general",
            c.t(Tr::NavGeneral),
            None,
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(28.))
                .children(self.banner(&c, cx))
                .child(self.input_group(&c, cx))
                .child(self.audio_group(&c, cx))
                .child(self.system_group(&c, cx))
                .child(self.setup_group(&c, cx)),
        )
    }
}
