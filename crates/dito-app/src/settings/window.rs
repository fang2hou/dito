use std::sync::Arc;

use gpui_kit::*;

use super::Page;
use super::shell::Settings;
use crate::app_shell::{self, ShellWindow};
use crate::state::{AppGlobal, AppState};
#[cfg(feature = "screenshots")]
use crate::ui;
use crate::ui::Root;

type SettingsHandle = ShellWindow<Settings>;

pub fn handle(cx: &App) -> Option<(WindowHandle<Root>, Entity<Settings>)> {
    cx.try_global::<SettingsHandle>().map(|h| (h.window, h.view.clone()))
}

#[cfg(feature = "screenshots")]
pub fn shell_motion(cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<SettingsHandle>().map(|h| h.view.read(cx).chrome.motion())
}

#[cfg(feature = "screenshots")]
pub fn page_motion(page: Page, cx: &App) -> Option<ui::ScopeHandle> {
    cx.try_global::<SettingsHandle>().map(|h| h.view.read(cx).page_motion(page, cx))
}

pub fn is_open(cx: &App) -> bool {
    cx.has_global::<SettingsHandle>()
}

/// Opens Settings on the Models page with the speech-detection tab selected: where the missing model
/// the transcription page asked for lives.
pub fn open_vad_models(st: Arc<AppState>, cx: &mut App) {
    open(st, Page::Models, cx);
    let Some(view) = cx.try_global::<SettingsHandle>().map(|h| h.view.clone()) else { return };
    view.update(cx, |settings, cx| settings.show_vad(cx));
}

/// Opens Settings on the Models page with the text model tab selected: where the model behind the chat lives.
pub fn open_text_model(st: Arc<AppState>, cx: &mut App) {
    open(st, Page::Models, cx);
    let Some(view) = cx.try_global::<SettingsHandle>().map(|h| h.view.clone()) else { return };
    view.update(cx, |settings, cx| settings.show_text(cx));
}

pub fn open(st: Arc<AppState>, page: Page, cx: &mut App) {
    let foreground = !st.opts.is_headless();
    if !show_in_existing_window(Some(page), foreground, cx) {
        open_new_window(st.clone(), page, cx);
    }
    if foreground {
        crate::dock::sync(cx);
        cx.activate(true);
    }
}

pub(crate) fn bring_to_front(cx: &mut App) -> bool {
    show_in_existing_window(None, true, cx)
}

/// `false` when there is none.
fn show_in_existing_window(page: Option<Page>, foreground: bool, cx: &mut App) -> bool {
    let Some(h) = cx.try_global::<SettingsHandle>() else { return false };
    let (window, view) = (h.window, h.view.clone());
    // Navigating from inside the settings window (a button click): that window is mid-update and gpui rejects nested
    // window updates, so update the view directly. Notify re-renders the window, and a clicked window is by definition
    // already active. Calling window.update here would report "window not found" and open a duplicate window.
    if cx.active_window().is_some_and(|active| active.window_id() == window.window_id()) {
        if let Some(page) = page {
            view.update(cx, |s, cx| s.set_page(page, cx));
        }
        return true;
    }
    window
        .update(cx, |_, window, cx| {
            if let Some(page) = page {
                view.update(cx, |s, cx| s.set_page(page, cx));
            }
            if foreground {
                window.activate_window();
            }
        })
        .is_ok()
}

fn open_new_window(st: Arc<AppState>, page: Page, cx: &mut App) {
    let default_size = size(px(1120.), px(860.));
    app_shell::open_shell_window(
        st,
        "settings",
        default_size,
        |st, window, cx| Settings::new(st, page, window, cx),
        cx,
    );
}

pub fn global_state(cx: &App) -> Arc<AppState> {
    cx.global::<AppGlobal>().0.clone()
}
