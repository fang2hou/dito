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

/// Where the pointer rests in a frozen HUD: off it, over the zone alone, or over one of its two buttons.
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
    /// A frozen HUD for screenshots: `t` seconds into `state`, with the pointer at `hover`. `came_from` is the
    /// state before, which sets the width the capsule is still widening from.
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
            width_from: capsule::rest_width(came_from.as_ref().unwrap_or(&state)),
            hover_zone,
            pointer_in_zone: hover_zone,
            hover_btn,
            freeze: Some(t),
            error_reason: Spring { position: f32::from(hover_btn.is_some()), velocity: 0. },
            ..Self::idle(st, state)
        }
    }

    /// The real renderer and animation clock, held in a processing stage for motion capture.
    pub fn motion_preview(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.state = HudState::Recognizing;
        view.width_from = CAPSULE_W;
        view.opened -= Duration::from_millis(POP_MS);
        view
    }

    pub fn outcome_preview(st: Arc<AppState>, copy: bool, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.state = if copy { HudState::Copied } else { HudState::Inserted };
        view.previous = Some(HudState::Recognizing);
        view.carry = Some(0.485);
        view.width_from = CAPSULE_W;
        view.opened -= Duration::from_millis(POP_MS);
        view
    }

    /// Same live filter/renderer with a repeatable input level for visual verification.
    pub fn waveform_preview(st: Arc<AppState>, gain: f32, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(st, cx);
        view.state = HudState::Listening { mode: RecordMode::Toggle };
        view.width_from = CAPSULE_W;
        view.hover_zone = true;
        view.wave_preview_gain = Some(gain);
        view.opened -= Duration::from_millis(POP_MS);
        view
    }
}
