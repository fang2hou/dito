//! Opening the wizard window.

use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::state::AppState;
use crate::ui::Root;

use super::Onboarding;

pub struct WizardWindow(pub WindowHandle<Root>);
impl Global for WizardWindow {}

#[cfg(feature = "screenshots")]
pub fn open_with_view(st: Arc<AppState>, cx: &mut App) -> Option<(WindowHandle<Root>, Entity<Onboarding>)> {
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
            cx.new(|cx| Root::new(view, window, cx))
        })
        .ok()?;
    slot.map(|v| (h, v))
}

/// The wizard keeps its global after the window closes, so ask the app whether the window is still there.
pub fn is_open(cx: &App) -> bool {
    cx.try_global::<WizardWindow>().is_some_and(|h| cx.windows().iter().any(|w| w.window_id() == h.0.window_id()))
}

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
        let view = cx.new(|cx| {
            let mut wizard = Onboarding::new(st, window, cx);
            wizard.begin_intro(cx);
            wizard
        });
        cx.new(|cx| Root::new(view, window, cx))
    }) {
        Ok(h) => cx.set_global(WizardWindow(h)),
        Err(e) => tracing::error!("could not open onboarding: {e}"),
    }
    if foreground {
        crate::dock::sync(cx);
        cx.activate(true);
    }
}
