use std::sync::Arc;

use gpui_kit::*;

use super::shell::Tools;
#[cfg(feature = "screenshots")]
use crate::app_shell::NavHost;
use crate::app_shell::traffic_light_position;
use crate::state::AppState;
#[cfg(feature = "screenshots")]
use crate::ui;
use crate::ui::Root;

struct ToolsHandle {
    window: WindowHandle<Root>,
    view: Entity<Tools>,
}

impl Global for ToolsHandle {}

/// Keeps the tools window-close observer alive independently of the handle it watches: the callback
/// removes that handle, so storing the subscription inside it would unsubscribe itself mid-callback.
/// Replaced (and so dropped, safely outside any callback) by the next tools window.
struct ToolsCloseWatch {
    _subscription: gpui_kit::Subscription,
}

impl Global for ToolsCloseWatch {}

#[cfg(feature = "screenshots")]
pub fn handle(cx: &App) -> Option<(WindowHandle<Root>, Entity<Tools>)> {
    cx.try_global::<ToolsHandle>().map(|h| (h.window, h.view.clone()))
}

#[cfg(feature = "screenshots")]
pub fn shell_motion(cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<ToolsHandle>().map(|h| h.view.read(cx).chrome().motion())
}

#[cfg(feature = "screenshots")]
pub fn page_motion(cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<ToolsHandle>().map(|h| h.view.read(cx).transcription().read(cx).motion_scope())
}

pub fn is_open(cx: &App) -> bool {
    cx.has_global::<ToolsHandle>()
}

pub fn open(st: Arc<AppState>, cx: &mut App) {
    let foreground = !st.opts.is_headless();
    if !show_in_existing_window(foreground, cx) {
        open_new_window(st.clone(), cx);
    }
    if foreground {
        crate::dock::sync(cx);
        cx.activate(true);
    }
}

pub(crate) fn bring_to_front(cx: &mut App) -> bool {
    show_in_existing_window(true, cx)
}

/// `false` when there is none.
fn show_in_existing_window(foreground: bool, cx: &mut App) -> bool {
    let Some(h) = cx.try_global::<ToolsHandle>() else { return false };
    let (window, view) = (h.window, h.view.clone());
    // Navigating from inside the tools window: that window is mid-update and gpui rejects nested
    // window updates, so update the view directly. Notify re-renders the window, and a clicked
    // window is by definition already active.
    if cx.active_window().is_some_and(|active| active.window_id() == window.window_id()) {
        view.update(cx, |tools, cx| tools.activate(cx));
        return true;
    }
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |tools, cx| tools.activate(cx));
            if foreground {
                window.activate_window();
            }
        })
        .is_ok()
}

fn open_new_window(st: Arc<AppState>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1120.), px(680.)), cx);
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
        let view = cx.new(|cx| Tools::new(st, window, cx));
        view_slot = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    });
    match (res, view_slot) {
        (Ok(window), Some(view)) => {
            cx.set_global(ToolsHandle { window, view });
            watch_close(window.window_id(), cx);
        }
        (Err(e), _) => tracing::error!("could not open tools: {e}"),
        _ => {}
    }
}

/// The global holds the view strongly, so closing the window alone would not release it. Both ids must match, the
/// closed window and the window of the handle currently installed, so neither a close of any other window nor an old
/// observer can touch a newer window's handle.
fn watch_close(id: WindowId, cx: &mut App) {
    let close = cx.on_window_closed(move |cx, closed| {
        if closed != id {
            return;
        }
        let Some(h) = cx.try_global::<ToolsHandle>() else { return };
        if h.window.window_id() != id {
            return;
        }
        let view = h.view.clone();
        cx.remove_global::<ToolsHandle>();
        view.update(cx, |tools, cx| tools.on_window_closed(cx));
    });
    cx.set_global(ToolsCloseWatch { _subscription: close });
}
