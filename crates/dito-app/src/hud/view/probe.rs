//! Constructors that freeze or steer the HUD for screenshots and motion capture.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::RecordMode;
use dito_core::controller::{ErrorAction, HudState};
use gpui_kit::prelude::*;

use crate::hud::capsule::{self, CAPSULE_W};
use crate::hud::motion::Spring;
use crate::state::AppState;

use super::{HudView, POP_MS};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PreviewHover {
    Away,
    Zone,
    /// The left button: cancel, or dismiss on an error.
    Dismiss,
    /// The right button: finish, or retry or settings on an error.
    Action,
}

impl HudView {
    /// Feed the native panel's polling path without moving the user's pointer.
    pub fn preview_pointer(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.pointer_update(x, y, cx);
    }

    pub fn entry_preview(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        let mut view = Self::waveform_preview(st, 0.06, cx);
        view.opened = std::time::Instant::now();
        view.hover_zone = false;
        view
    }

    /// `came_from` is the state before, which sets the width the capsule is still widening from.
    pub fn preview(
        st: Arc<AppState>,
        state: HudState,
        t: f32,
        hover: PreviewHover,
        came_from: Option<HudState>,
    ) -> Self {
        let hover_btn = match (&state, hover) {
            (HudState::Error { .. }, PreviewHover::Dismiss) => Some("hud-dismiss"),
            (HudState::Error { action: ErrorAction::Retry, .. }, PreviewHover::Action) => Some("hud-retry"),
            (HudState::Error { .. }, PreviewHover::Action) => Some("hud-settings"),
            (_, PreviewHover::Dismiss) => Some("hud-cancel"),
            (_, PreviewHover::Action) => Some("hud-finish"),
            _ => None,
        };
        let hover_zone = hover != PreviewHover::Away;
        Self {
            width_from: Spring { position: capsule::rest_width(came_from.as_ref().unwrap_or(&state)), velocity: 0. },
            hover_zone,
            pointer_in_zone: hover_zone,
            hover_btn,
            freeze: Some(t),
            error_reason: Spring { position: f32::from(hover_btn.is_some()), velocity: 0. },
            ..Self::idle(st, state)
        }
    }

    pub fn motion_preview(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.start_hidden_preview_clock(cx);
        view.state = HudState::Transcribing;
        view.start_processing(super::Fill::default());
        view.width_from = Spring { position: CAPSULE_W, velocity: 0. };
        view.opened -= Duration::from_millis(POP_MS);
        view
    }

    pub fn outcome_preview(st: Arc<AppState>, copy: bool, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.start_hidden_preview_clock(cx);
        view.state = if copy { HudState::Copied } else { HudState::Inserted };
        let previous = crate::hud::processing_motion::ProcessingMotion::transcribing(1., false);
        view.start_processing(previous.at(10.));
        view.width_from = Spring { position: CAPSULE_W, velocity: 0. };
        view.opened -= Duration::from_millis(POP_MS);
        view
    }

    /// A repeatable input level makes the live filter and renderer verifiable by eye.
    pub fn waveform_preview(st: Arc<AppState>, gain: f32, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.start_hidden_preview_clock(cx);
        view.state = HudState::Listening { mode: RecordMode::Toggle };
        view.width_from = Spring { position: CAPSULE_W, velocity: 0. };
        view.hover_zone = true;
        view.wave_preview_gain = Some(gain);
        view.opened -= Duration::from_millis(POP_MS);
        view
    }
    /// Hidden capture windows have no native display-link signal.
    fn start_hidden_preview_clock(&mut self, cx: &mut Context<Self>) {
        self.frame_clock_started = true;
        self.preview_clock = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                if view
                    .update(cx, |view, cx| {
                        if view.advance_clock() {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
}
