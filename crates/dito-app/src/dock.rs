//! The Dock icon. With "Show icon in Dock" off the app runs without one, so it is only a menu bar item. While a
//! window is open the icon returns, so the window can be found again with the Dock and Cmd-Tab, and it leaves when
//! the last window closes.

use std::sync::Arc;

use gpui_kit::*;

use crate::state::{AppGlobal, AppState};
use crate::{onboarding, settings};

/// Whether the app currently has a Dock icon (the Regular activation policy).
struct IconShown(bool);

impl Global for IconShown {}

/// Whether the app starts without a Dock icon. The policy has to be set before the run loop starts, or the icon
/// flashes in the Dock during launch.
pub fn starts_hidden(st: &AppState) -> bool {
    st.opts.is_headless() || !st.cfg.get().general.show_dock_icon
}

/// Track the setting and window closes; call once inside the running app.
pub fn run(st: Arc<AppState>, cx: &mut App) {
    if st.opts.is_headless() {
        return;
    }
    cx.set_global(IconShown(st.cfg.get().general.show_dock_icon));
    cx.on_window_closed(|cx, _| sync_later(cx)).detach();
    let mut config_rx = st.cfg.subscribe();
    cx.spawn(async move |cx| {
        while config_rx.changed().await.is_ok() {
            cx.update(sync);
        }
    })
    .detach();
}

/// Show or hide the icon to match the setting and the open windows. Call it after opening a window, before
/// activating the app: a Regular app has to be activated by hand for its menu bar to appear.
pub fn sync(cx: &mut App) {
    let Some(shown) = cx.try_global::<IconShown>().map(|s| s.0) else { return };
    let setting = cx.global::<AppGlobal>().0.cfg.get().general.show_dock_icon;
    let wanted = setting || settings::is_open(cx) || onboarding::is_open(cx);
    if wanted == shown {
        return;
    }
    cx.set_global(IconShown(wanted));
    cx.set_activation_policy(if wanted { ActivationPolicy::Regular } else { ActivationPolicy::Accessory });
}

/// Switching policy inside the close callback makes AppKit report an active-status change for the window that was
/// just removed, so it runs on the next turn instead.
fn sync_later(cx: &mut App) {
    cx.spawn(async move |cx| cx.update(sync)).detach();
}
