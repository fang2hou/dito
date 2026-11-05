//! The native HUD window: bounds, level, shadow suppression and show/hide lifecycle.

use std::sync::Arc;

use dito_core::config::HudPosition;
use dito_core::controller::HudState;
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
    _nap: dito_platform::system::NapGuard,
}

impl Global for HudWindow {}

fn open_window(st: Arc<AppState>, cx: &mut App) {
    if cx.has_global::<HudWindow>() {
        return;
    }
    let Some(display) = cx.primary_display() else { return };
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
    let opts = WindowOptions {
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
    };
    let settings = st.clone();
    match cx.open_window(opts, |window, cx| {
        drop_shadow(window);
        always_on_top(window);
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::AppKit(h) = handle.as_raw()
        {
            // SAFETY: PopUp creates an NSPanel; the live view is on the main thread.
            unsafe { dito_platform::system::configure_nonactivating_panel(h.ns_view) };
        }
        cx.new(|cx| HudView::new(st, cx))
    }) {
        Ok(handle) => {
            cx.set_global(HudWindow {
                window: handle.into(),
                _nap: dito_platform::system::NapGuard::new("dictation-hud"),
            });
            cx.spawn(async move |cx| {
                let mut position = None;
                let mut follow = dito_platform::system::HudFollowState::default();
                loop {
                    let ok = handle.update(cx, |view, window, cx| {
                        let Ok(h) = HasWindowHandle::window_handle(window) else { return };
                        let RawWindowHandle::AppKit(h) = h.as_raw() else { return };
                        let cfg = settings.cfg.get();
                        // A lower error bubble needs its shadow and spring overshoot above the Dock.
                        let lower_room = view.needs_lower_room();
                        let clearance =
                            if lower_room { cfg.general.hud_clearance().max(12) } else { cfg.general.hud_clearance() };
                        let current = (cfg.general.hud_position, clearance);
                        if position != Some(current) {
                            position = Some(current);
                            follow = dito_platform::system::HudFollowState::default();
                            if current.0 == HudPosition::Centered {
                                unsafe { dito_platform::system::hud_center_above_dock(h.ns_view, current.1) };
                            }
                        }
                        // SAFETY: this is the live window view on GPUI's main thread.
                        if let Some((x, y, pressed)) = unsafe { dito_platform::system::hud_pointer(h.ns_view) } {
                            let interactive = view.pointer_update(x, y, cx);
                            // Keep the down/up pair on the same target while a button is held.
                            if !pressed {
                                unsafe { dito_platform::system::hud_set_click_through(h.ns_view, !interactive) };
                            }
                            if current.0 == HudPosition::Pointer {
                                unsafe {
                                    dito_platform::system::hud_follow_pointer(
                                        h.ns_view,
                                        &mut follow,
                                        interactive || pressed,
                                        lower_room,
                                    )
                                };
                            }
                        }
                    });
                    if ok.is_err() {
                        break;
                    }
                    cx.background_executor().timer(std::time::Duration::from_millis(8)).await;
                }
            })
            .detach();
        }
        Err(e) => tracing::error!("could not open the HUD: {e}"),
    }
}

/// Suppress the OS window shadow; each HUD surface draws its own small, controlled shadow.
fn drop_shadow(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::AppKit(h) = handle.as_raw()
    {
        // SAFETY: the view belongs to the window that was just created on the main thread.
        unsafe { dito_platform::system::remove_window_shadow(h.ns_view) };
    }
}

/// gpui has no window-level option, and its floating window kind drops the hover tracking area it
/// installs for normal windows, so raise the native window to the macOS status level directly:
/// above every app's windows and panels, below menus and tooltips.
fn always_on_top(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::AppKit(h) = handle.as_raw()
    {
        // SAFETY: the view belongs to the window that was just created on the main thread.
        unsafe {
            if let Some(level) = dito_platform::system::set_status_window_level(h.ns_view) {
                tracing::debug!(level, "hud window raised to the status level");
            }
        }
    }
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
