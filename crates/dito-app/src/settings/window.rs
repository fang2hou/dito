use std::sync::Arc;

use gpui_kit::*;

use super::Page;
use super::page::traffic_light_position;
use super::shell::Settings;
use crate::state::{AppGlobal, AppState};
#[cfg(feature = "screenshots")]
use crate::ui;

struct SettingsHandle {
    window: WindowHandle<gpui_kit::component::Root>,
    view: Entity<Settings>,
}

impl Global for SettingsHandle {}

/// Keeps the settings window-close observer alive independently of the handle it watches: the
/// callback removes that handle, so storing the subscription inside it would unsubscribe itself
/// mid-callback. Replaced (and so dropped, safely outside any callback) by the next settings window.
struct SettingsCloseWatch {
    _subscription: gpui_kit::Subscription,
}

impl Global for SettingsCloseWatch {}

#[cfg(feature = "screenshots")]
pub fn handle(cx: &App) -> Option<(WindowHandle<gpui_kit::component::Root>, Entity<Settings>)> {
    cx.try_global::<SettingsHandle>().map(|h| (h.window, h.view.clone()))
}

/// The settings shell's motion scope, for the screenshot harness.
#[cfg(feature = "screenshots")]
pub fn shell_motion(cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<SettingsHandle>().map(|h| h.view.read(cx).motion.handle())
}

/// One page's motion scope, for the screenshot harness.
#[cfg(feature = "screenshots")]
pub fn page_motion(page: Page, cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<SettingsHandle>().map(|h| {
        let view = h.view.read(cx);
        match page {
            Page::General => view.general.read(cx).motion_scope(),
            Page::Models => view.models.read(cx).motion_scope(),
            Page::Dictionary => view.dictionary.read(cx).motion_scope(),
            Page::Prompts => view.prompts.read(cx).motion_scope(),
            Page::History => view.history.read(cx).motion_scope(),
            Page::Stats => view.stats.read(cx).motion_scope(),
            Page::Status => view.status.read(cx).motion_scope(),
        }
    })
}

/// Open (or focus) the settings window on `page`.
pub fn open(st: Arc<AppState>, page: Page, cx: &mut App) {
    let foreground = !st.opts.is_headless();
    if !show_in_existing_window(page, foreground, cx) {
        open_new_window(st.clone(), page, cx);
    }
    if foreground {
        cx.activate(true);
    }
}

/// Bring the open settings window to `page`; `false` when there is none.
fn show_in_existing_window(page: Page, foreground: bool, cx: &mut App) -> bool {
    let Some(h) = cx.try_global::<SettingsHandle>() else { return false };
    let (window, view) = (h.window, h.view.clone());
    // Navigating from inside the settings window (a button click): that window is mid-update
    // and gpui rejects nested window updates, so update the view directly — notify re-renders
    // the window, and a clicked window is by definition already active. Doing window.update
    // here would report "window not found" and open a duplicate window.
    if cx.active_window().is_some_and(|active| active.window_id() == window.window_id()) {
        view.update(cx, |s, cx| s.set_page(page, cx));
        return true;
    }
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |s, cx| s.set_page(page, cx));
            if foreground {
                window.activate_window();
            }
        })
        .is_ok()
}

fn open_new_window(st: Arc<AppState>, page: Page, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1120.), px(860.)), cx);
    let opts = WindowOptions {
        inactive_frame_interval: None,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Dito".into()),
            appears_transparent: true,
            traffic_light_position: Some(traffic_light_position(false)),
        }),
        window_min_size: Some(size(px(900.), px(640.))),
        focus: !st.opts.is_headless(),
        show: !st.opts.is_headless(),
        ..Default::default()
    };
    let mut view_slot = None;
    let res = cx.open_window(opts, |window, cx| {
        let view = cx.new(|cx| Settings::new(st, page, window, cx));
        view_slot = Some(view.clone());
        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
    });
    match (res, view_slot) {
        (Ok(window), Some(view)) => {
            cx.set_global(SettingsHandle { window, view });
            watch_close(window.window_id(), cx);
        }
        (Err(e), _) => tracing::error!("could not open settings: {e}"),
        _ => {}
    }
}

/// The global holds the view strongly, so closing the window alone would not release it. Watch
/// for exactly this window's close: stop the page's data lifecycle, invalidate every shell and
/// page scope, and drop the handle. Both ids must match — the closed window and the window of the
/// handle currently installed — so neither a close of any other window nor an old observer can
/// touch a newer window's handle.
fn watch_close(id: WindowId, cx: &mut App) {
    let close = cx.on_window_closed(move |cx, closed| {
        if closed != id {
            return;
        }
        let Some(h) = cx.try_global::<SettingsHandle>() else { return };
        if h.window.window_id() != id {
            return;
        }
        let view = h.view.clone();
        cx.remove_global::<SettingsHandle>();
        view.update(cx, |s, cx| s.on_window_closed(cx));
    });
    cx.set_global(SettingsCloseWatch { _subscription: close });
}

pub fn global_state(cx: &App) -> Arc<AppState> {
    cx.global::<AppGlobal>().0.clone()
}
