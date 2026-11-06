//! The native HUD window: bounds, level, shadow suppression and show/hide lifecycle.

use std::sync::Arc;

use dito_core::config::HudPosition;
use dito_core::controller::HudState;
use dito_platform::system::{ErrorBubble, HudFollowState, HudNativeWindow, NapGuard, PointerContact};
use gpui_kit::prelude::*;
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::state::AppState;

use super::px;
use super::view::HudView;

pub const WIN_W: f32 = 273.0;
pub const WIN_H: f32 = 180.0;
/// Padding keeps the capsule and its controls away from the transparent window edges.
pub const WIN_PAD: f32 = 10.0;

struct HudWindow {
    window: AnyWindowHandle,
    /// Held while the HUD is on screen (dropped when the window closes): keeps macOS from
    /// App-Napping this background app mid-dictation, which throttled the HUD's frame commits.
    _nap: NapGuard,
}

impl Global for HudWindow {}

fn window_options(st: &AppState, cx: &App) -> Option<WindowOptions> {
    let display = cx.primary_display()?;
    let screen = display.bounds();
    let visible = display.visible_bounds();
    // WIN_PAD reserves space around the content, so the controls are not clipped at the
    // window edge. The extra 30 pt below the capsule reserves the countdown; shifting the
    // window down by the same amount keeps the capsule at its existing screen position.
    let (ww, wh) = (WIN_W + 2. * WIN_PAD, WIN_H + 2. * WIN_PAD);
    let origin = point(
        screen.origin.x + (screen.size.width - px(ww)) / 2.,
        visible.origin.y + visible.size.height - px(wh) + px(WIN_PAD + 5.)
            - px(st.cfg.get().general.hud_clearance() as f32),
    );
    Some(WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, size(px(ww), px(wh))))),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        display_id: Some(display.id()),
        ..Default::default()
    })
}

fn open_window(st: Arc<AppState>, cx: &mut App) {
    if cx.has_global::<HudWindow>() {
        return;
    }
    let Some(opts) = window_options(&st, cx) else { return };
    let settings = st.clone();
    match cx.open_window(opts, |window, cx| {
        configure_native_window(window);
        cx.new(|cx| HudView::new(st, cx))
    }) {
        Ok(handle) => {
            cx.set_global(HudWindow { window: handle.into(), _nap: NapGuard::new("dictation-hud") });
            spawn_placement_loop(handle, settings, cx);
        }
        Err(e) => tracing::error!("could not open the HUD: {e}"),
    }
}

/// Every 8 ms: keep the window where the settings put it, and click-through except over the controls.
fn spawn_placement_loop(handle: WindowHandle<HudView>, settings: Arc<AppState>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut placement = Placement::default();
        loop {
            let ok = handle.update(cx, |view, window, cx| placement.tick(&settings, view, window, cx));
            if ok.is_err() {
                break;
            }
            cx.background_executor().timer(std::time::Duration::from_millis(8)).await;
        }
    })
    .detach();
}

#[derive(Default)]
struct Placement {
    applied: Option<(HudPosition, i32)>,
    follow: HudFollowState,
}

impl Placement {
    fn tick(&mut self, settings: &AppState, view: &mut HudView, window: &mut Window, cx: &mut Context<HudView>) {
        let Some(native) = native_window(window) else { return };
        let cfg = settings.cfg.get();
        // A lower error bubble needs its shadow and spring overshoot above the Dock.
        let lower_room = view.needs_lower_room();
        let clearance = if lower_room { cfg.general.hud_clearance().max(12) } else { cfg.general.hud_clearance() };
        let current = (cfg.general.hud_position, clearance);
        if self.applied != Some(current) {
            self.applied = Some(current);
            self.follow = HudFollowState::default();
            if current.0 == HudPosition::Centered {
                native.center_above_dock(current.1);
            }
        }
        let Some(pointer) = native.pointer() else { return };
        let interactive = view.pointer_update(pointer.x, pointer.y, cx);
        // Keep the down/up pair on the same target while a button is held.
        if !pointer.button_down {
            native.set_click_through(!interactive);
        }
        if current.0 == HudPosition::Pointer {
            let contact =
                if interactive || pointer.button_down { PointerContact::OnControls } else { PointerContact::Away };
            let bubble = if lower_room { ErrorBubble::Lowered } else { ErrorBubble::Hidden };
            native.follow_pointer(&mut self.follow, contact, bubble);
        }
    }
}

/// The HUD's native window: no OS shadow (each HUD surface draws its own small, controlled one), raised above
/// every app's windows and panels but below menus and tooltips, and never made key by a click.
fn configure_native_window(window: &Window) {
    let Some(native) = native_window(window) else { return };
    native.remove_shadow();
    if let Some(level) = native.raise_to_status_level() {
        tracing::debug!(level, "hud window raised to the status level");
    }
    native.keep_panel_inactive();
}

fn native_window(window: &Window) -> Option<HudNativeWindow<'_>> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(h) = handle.as_raw() else { return None };
    // SAFETY: the view belongs to `window`, which the borrow keeps alive.
    unsafe { HudNativeWindow::from_ns_view(h.ns_view) }
}

fn close_window(cx: &mut App) {
    if let Some(h) = cx.try_global::<HudWindow>().map(|h| h.window) {
        let _ = h.update(cx, |_, window, _| window.remove_window());
        cx.remove_global::<HudWindow>();
    }
}

/// Show/hide the HUD window with the dictation state, play cues, and keep Esc consumption in sync.
pub fn run(st: Arc<AppState>, cx: &mut App) {
    let mut rx = st.dictation.hud();
    cx.spawn(async move |cx| {
        while rx.changed().await.is_ok() {
            let now = rx.borrow_and_update().clone();
            if let HudState::Error { kind, detail, .. } = &now {
                tracing::warn!("hud error: {kind:?} — {detail}");
            }
            let visible = now != HudState::Hidden;
            st.hotkeys.set_swallow_escape(visible);
            let st2 = st.clone();
            cx.update(|cx| {
                if visible {
                    open_window(st2, cx);
                } else {
                    close_window(cx)
                }
            });
        }
    })
    .detach();
}
