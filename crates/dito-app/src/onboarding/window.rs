//! Opening the wizard window: first run, restart from Settings, and the preview handle.

use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::state::AppState;

use super::Onboarding;

pub struct WizardWindow(pub WindowHandle<gpui_kit::component::Root>);
impl Global for WizardWindow {}

/// Entity handle for previews.
#[cfg(feature = "screenshots")]
pub fn open_with_view(
    st: Arc<AppState>,
    cx: &mut App,
) -> Option<(WindowHandle<gpui_kit::component::Root>, Entity<Onboarding>)> {
    let bounds = Bounds::centered(None, size(px(1040.), px(720.)), cx);
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Dito".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(20.), px(20.))),
        }),
        focus: false,
        show: false,
        ..Default::default()
    };
    let mut slot = None;
    let h = cx
        .open_window(opts, |window, cx| {
            let view = cx.new(|cx| Onboarding::new(st, window, cx));
            slot = Some(view.clone());
            cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
        })
        .ok()?;
    slot.map(|v| (h, v))
}

/// Open the wizard again (Settings ▸ 初期設定), or bring the one that is already open to the front.
pub fn restart(st: Arc<AppState>, cx: &mut App) {
    if let Some(h) = cx.try_global::<WizardWindow>().map(|h| h.0)
        && h.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        cx.activate(true);
        return;
    }
    open(st, cx);
}

pub fn open(st: Arc<AppState>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1040.), px(720.)), cx);
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Dito".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(20.), px(20.))),
        }),
        is_resizable: false,
        focus: !st.opts.is_headless(),
        show: !st.opts.is_headless(),
        ..Default::default()
    };
    let foreground = !st.opts.is_headless();
    match cx.open_window(opts, |window, cx| {
        let view = cx.new(|cx| Onboarding::new(st, window, cx));
        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
    }) {
        Ok(h) => cx.set_global(WizardWindow(h)),
        Err(e) => tracing::error!("could not open onboarding: {e}"),
    }
    if foreground {
        cx.activate(true);
    }
}
