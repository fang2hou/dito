use std::sync::Arc;

use gpui_kit::*;

use super::shell::Tools;
#[cfg(feature = "screenshots")]
use crate::app_shell::NavHost;
use crate::app_shell::{self, ShellWindow};
use crate::state::AppState;
#[cfg(feature = "screenshots")]
use crate::ui;
use crate::ui::Root;

type ToolsHandle = ShellWindow<Tools>;

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
    app_shell::open_shell_window(st, "tools", size(px(1120.), px(680.)), Tools::new, cx);
}
