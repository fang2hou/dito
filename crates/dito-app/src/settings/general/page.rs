use std::cell::Cell;
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
    /// Loaded when the microphone list opens: enumerating devices at launch would touch the audio system (and any
    /// nearby iPhone) before the user asked for a microphone.
    pub(super) devices: Vec<InputDevice>,
    /// Opening the list again replaces it, and dropping it ends the loop.
    _mic_poll: Option<Task<()>>,
    _permission_poll: Task<()>,
    pub(super) capture: CaptureState,
    pub(super) accessibility_ok: bool,
    pub(super) hud_preview: Option<Entity<StylePreview>>,
    pub(super) hud_height: Entity<HudHeightSlider>,
    pub(super) motion: ui::MotionScope,
    /// Set during a render that drew a running fade, so the view asks for the next frame.
    pub(super) fading: Cell<bool>,
}

impl GeneralPage {
    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);

        ui::observe(st.hotkeys.capture.subscribe(), cx, |t, capture, cx| {
            t.capture = capture;
            cx.notify();
        });

        ui::notify_on_change(st.hotkeys.unavailable.subscribe(), cx);

        // Permissions change outside the app; poll lightly while the window exists.
        let permission_poll = cx.spawn(async move |this, cx| {
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
        });

        ui::observe(st.dictation.hud(), cx, |page, state, cx| {
            if state != HudState::Hidden {
                page.cancel_hud_preview(cx);
            }
        });

        let hud_height = cx.new(|cx| HudHeightSlider::new(st.clone(), cx));
        Self {
            motion: ui::MotionScope::new(),
            fading: Cell::new(false),
            hud_preview: None,
            hud_height,
            devices: Vec::new(),
            _mic_poll: None,
            _permission_poll: permission_poll,
            st,
            open: None,
            capture: CaptureState::Idle,
            accessibility_ok: permissions::status(Permission::Accessibility) == Status::Granted,
        }
    }

    pub(super) fn toggle(&self, cx: &mut Context<Self>, id: &'static str) -> Cb {
        cbk(cx, move |this: &mut Self, _, cx| {
            this.open = if this.open == Some(id) { None } else { Some(id) };
            if this.open == Some("mic") && (!this.st.opts.mock || this.st.opts.real_mic) {
                this.devices = dito_audio::list_input_devices();
                // Headsets connect a few seconds after they are put on: keep the list current while it is shown.
                this._mic_poll = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(MIC_POLL).await;
                        let devices = cx.background_executor().spawn(async { dito_audio::list_input_devices() }).await;
                        let keep = this.update(cx, |t, cx| {
                            if t.open != Some("mic") {
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
                }));
            } else {
                this._mic_poll = None;
            }
            cx.notify();
        })
    }
}

impl GeneralPage {
    pub(in crate::settings) fn reset_motion(&mut self) {
        self.motion.reset();
    }
}

impl Render for GeneralPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Own scope: after a page switch the controls mount statically instead of sliding to values remembered from an
        // earlier visit.
        self.fading.set(false);
        ui::render_with_scope(self.motion.handle(), || {
            let page = self.view(cx).into_any_element();
            if self.fading.get() {
                window.request_animation_frame();
            }
            page
        })
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
                .gap(px(16.))
                .children(self.banner(&c, cx))
                .child(self.input_group(&c, cx))
                .child(self.speech_recognition_group(&c, cx))
                .child(self.command_group(&c, cx))
                .child(self.hud_group(&c, cx))
                .child(self.system_group(&c, cx))
                .child(self.setup_group(&c, cx)),
        )
    }
}
