//! The native HUD window.

use std::sync::Arc;

use dito_core::config::HudPosition;
use dito_core::controller::HudState;
use dito_platform::system::{
    HudFollowState, HudNativeWindow, HudWindowMove, NapGuard, PointerContact, give_front_back, pointer_location,
    source_app_in_front,
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::state::AppState;
use crate::ui::{self, Root};

use super::px;
use super::view::{HudView, KeyStep};

/// The window is fixed in size. It is bottom anchored, so the capsule keeps its screen position; the room above it
/// (350 pt from the window's top to the capsule) holds the command windows, their action buttons and a tooltip, and
/// the width holds the widest window (320 pt) with its shadow.
pub const WIN_W: f32 = 340.0;
pub const WIN_H: f32 = 408.0;
pub const WIN_PAD: f32 = 10.0;

/// A command that writes waits at most 400 ms for the app that had the front to get it back.
const FRONT_POLLS: u32 = 40;
const FRONT_POLL: std::time::Duration = std::time::Duration::from_millis(10);

struct HudWindow {
    window: AnyWindowHandle,
    /// Held while the HUD is on screen (dropped when the window closes): keeps macOS from App-Napping this background
    /// app mid-dictation, which throttled the HUD's frame commits.
    _nap: NapGuard,
    /// The placement loop; dropping it with the window cancels it.
    _placement: Task<()>,
}

impl Global for HudWindow {}

fn window_options(st: &AppState, cx: &mut App) -> Option<WindowOptions> {
    let anchor = crate::onboarding::try_anchor(cx);
    tracing::debug!(?anchor, "opening HUD at its active target");
    let display = anchor
        .map(|p| (f32::from(p.x), f32::from(p.y)))
        .or_else(pointer_location)
        .and_then(|(x, y)| cx.displays().into_iter().find(|display| display.bounds().contains(&point(px(x), px(y)))))
        .or_else(|| cx.primary_display())?;
    let screen = display.bounds();
    let visible = display.visible_bounds();
    // WIN_PAD reserves space around the content, so the controls are not clipped at the window edge. The extra 30 pt
    // below the capsule reserves the countdown; shifting the window down by the same amount keeps the capsule at its
    // existing screen position.
    let (ww, wh) = (WIN_W + 2. * WIN_PAD, WIN_H + 2. * WIN_PAD);
    let origin = anchor.map(|p| point(p.x - px(ww / 2.), p.y - px(wh - 45.))).unwrap_or_else(|| {
        point(
            screen.origin.x + (screen.size.width - px(ww)) / 2.,
            visible.origin.y + visible.size.height - px(wh) + px(WIN_PAD + 5.)
                - px(st.cfg.get().general.hud_clearance() as f32),
        )
    });
    Some(WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, size(px(ww), px(wh))))),
        titlebar: None,
        focus: false,
        inactive_frame_interval: None,
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
    let mut view_slot = None;
    // Its text fields need a `Root`, like every window with inputs; transparent so only the HUD's own surfaces show.
    match cx.open_window(opts, |window, cx| {
        configure_native_window(window);
        crate::theme::repaint_with_system(window);
        let view = cx.new(|cx| HudView::new(st, cx));
        view_slot = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx).bg(ui::transparent()))
    }) {
        Ok(handle) => {
            let view = view_slot.expect("the HUD view is built with its window");
            let placement = spawn_placement_loop(handle, view, settings, cx);
            cx.set_global(HudWindow {
                window: handle.into(),
                _nap: NapGuard::new("dictation-hud"),
                _placement: placement,
            });
        }
        Err(e) => tracing::error!("could not open the HUD: {e}"),
    }
}

/// Poll near the display cadence so a non-key panel can track hover and click-through.
fn spawn_placement_loop(
    handle: WindowHandle<Root>,
    view: Entity<HudView>,
    settings: Arc<AppState>,
    cx: &mut App,
) -> Task<()> {
    cx.spawn(async move |cx| {
        let mut placement = Placement::default();
        loop {
            let Ok((movement, key, interval)) = handle
                .update(cx, |_, window, cx| view.update(cx, |view, cx| placement.tick(&settings, view, window, cx)))
            else {
                break;
            };
            // AppKit calls back into gpui synchronously for both, so they run after the app borrow is released.
            if let Some(movement) = movement {
                movement.apply();
            }
            if let Some(step) = key {
                if let Some(change) = step.change {
                    change.apply();
                }
                // A command that writes into the source app goes out once the panel has given key status back and
                // that app has the front again.
                if let Some(command) = step.then {
                    for _ in 0..FRONT_POLLS {
                        if source_app_in_front() {
                            break;
                        }
                        cx.background_executor().timer(FRONT_POLL).await;
                    }
                    settings.dictation.send(command);
                }
            }
            cx.background_executor().timer(interval).await;
        }
    })
}

#[derive(Default)]
struct Placement {
    applied: Option<(HudPosition, i32)>,
    follow: HudFollowState,
    /// What the hotkey tap was last told about composing text in a field.
    composing: bool,
}

impl Placement {
    fn tick(
        &mut self,
        settings: &AppState,
        view: &mut HudView,
        window: &mut Window,
        cx: &mut Context<HudView>,
    ) -> (Option<HudWindowMove>, Option<KeyStep>, std::time::Duration) {
        let fallback = std::time::Duration::from_millis(8);
        let (accepts, then) = view.sync_key_status(window, cx);
        let composing = view.composing(window, cx);
        if composing != self.composing {
            self.composing = composing;
            settings.hotkeys.set_escape_to_ime(composing);
        }
        let Some(native) = native_window(window) else { return (None, None, fallback) };
        let cfg = settings.cfg.get();
        let clearance = cfg.general.hud_clearance();
        let current = (cfg.general.hud_position, clearance);
        if self.applied != Some(current) {
            self.applied = Some(current);
            self.follow = HudFollowState::default();
        }
        let interval =
            window.refresh_interval(cx).unwrap_or(fallback).clamp(std::time::Duration::from_millis(4), fallback);
        let key = (accepts.is_some() || then.is_some())
            .then(|| KeyStep { change: accepts.map(|accepts| native.set_accepts_key(accepts)), then });
        let Some(pointer) = native.pointer() else { return (None, key, interval) };
        let interactive = view.pointer_update(pointer.x, pointer.y, cx);
        // Keep the down/up pair on the same target while a button is held.
        if !pointer.button_down {
            native.set_click_through(!interactive);
        }
        let anchor = crate::onboarding::try_anchor(cx);
        let movement = if let Some(anchor) = anchor {
            self.follow = HudFollowState::default();
            native.anchor_capsule(f32::from(anchor.x), f32::from(anchor.y))
        } else {
            match current.0 {
                // A drag that crosses screens moves the HUD only once the button is released.
                HudPosition::Centered if pointer.button_down => None,
                HudPosition::Centered => native.center_above_dock(clearance),
                HudPosition::Pointer => {
                    let contact = if interactive || pointer.button_down {
                        PointerContact::OnControls
                    } else {
                        PointerContact::Away
                    };
                    native.follow_pointer(&mut self.follow, contact)
                }
            }
        };
        (movement, key, interval)
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

pub(super) fn native_window(window: &Window) -> Option<HudNativeWindow<'_>> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(h) = handle.as_raw() else { return None };
    // SAFETY: the view belongs to `window`, which the borrow keeps alive.
    unsafe { HudNativeWindow::from_ns_view(h.ns_view) }
}

#[cfg(feature = "screenshots")]
pub(crate) fn live_window(cx: &App) -> Option<AnyWindowHandle> {
    cx.try_global::<HudWindow>().map(|hud| hud.window)
}

fn close_window(cx: &mut App) {
    if let Some(h) = cx.try_global::<HudWindow>().map(|h| h.window) {
        let _ = h.update(cx, |_, window, _| window.remove_window());
        cx.remove_global::<HudWindow>();
        // The placement loop ends with the window, so a text window that closes while it holds the front hands it
        // back here.
        give_front_back();
    }
}

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
            if !visible {
                st.hotkeys.set_escape_to_ime(false);
            }
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
