//! Per-window state the shared chrome reads and updates: the rail mode, the measured health dot and
//! the window-wide motion scope.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::*;

use super::health;
use super::nav::{RAIL_COLLAPSE, RAIL_EXPAND, traffic_light_position};
use crate::settings::Ctx;
use crate::state::AppState;
use crate::theme;
use crate::ui;

pub(crate) struct Chrome {
    st: Arc<AppState>,
    rail: bool,
    health_dot_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The persistent shell scope: the sidebar's nav plate and hover live here for the window's
    /// whole life; page navigation resets page scopes but never this one.
    motion: ui::MotionScope,
}

impl Chrome {
    pub(crate) fn new(st: Arc<AppState>) -> Self {
        Self { st, rail: false, health_dot_bounds: Rc::new(Cell::new(None)), motion: ui::MotionScope::new() }
    }

    pub(crate) fn st(&self) -> &Arc<AppState> {
        &self.st
    }

    pub(crate) fn rail(&self) -> bool {
        self.rail
    }

    pub(crate) fn motion(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }

    pub(crate) fn health_dot_bounds(&self) -> Option<Bounds<Pixels>> {
        self.health_dot_bounds.get()
    }

    /// The live measurement slot the dot writes into; the card positions itself from it.
    pub(crate) fn health_dot_cell(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        self.health_dot_bounds.clone()
    }

    pub(crate) fn health_hovered(&self) -> bool {
        health::hovered(&self.motion.handle())
    }

    /// Drops every transient scope, so handles still held elsewhere (tests, screenshots) read static
    /// defaults instead of stale motion state.
    pub(crate) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    /// Hysteresis: the sidebar folds at RAIL_COLLAPSE and only unfolds past RAIL_EXPAND. Returns
    /// whether the mode changed, so the caller re-renders.
    fn follow_window_width(&mut self, window: &mut Window) -> bool {
        let vw = f32::from(window.viewport_size().width);
        let rail = if self.rail { vw < RAIL_EXPAND } else { vw < RAIL_COLLAPSE };
        if rail == self.rail {
            return false;
        }
        self.rail = rail;
        window.set_traffic_light_position(traffic_light_position(rail));
        true
    }

    /// What a render of the window's view starts with, before it builds any element: the rail mode
    /// follows the window, and the palette and language of this frame become the ambient ones.
    pub(crate) fn begin_frame<V: 'static>(&mut self, window: &mut Window, cx: &mut Context<V>) -> Ctx {
        let changed = self.follow_window_width(window);
        if changed {
            cx.notify();
        }
        let c = Ctx::new(&self.st, cx);
        theme::apply(&c.p, cx);
        c
    }
}
