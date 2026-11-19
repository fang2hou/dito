//! The one window each of Settings and Tools lives in: opening it, the global that finds it again,
//! and releasing that global when the window closes.

use std::marker::PhantomData;
use std::sync::Arc;

use gpui_kit::*;

use super::traffic_light_position;
use crate::state::AppState;
use crate::ui::Root;

/// The view a shell window holds.
pub(crate) trait ShellView: Render + Sized + 'static {
    fn on_window_closed(&mut self, cx: &mut Context<Self>);
}

/// The open window of `V`, if this global is set: there is at most one per view type.
pub(crate) struct ShellWindow<V> {
    pub(crate) window: WindowHandle<Root>,
    pub(crate) view: Entity<V>,
}

impl<V: 'static> Global for ShellWindow<V> {}

/// Keeps the window-close observer alive independently of the handle it watches: the callback
/// removes that handle, so storing the subscription inside it would unsubscribe itself mid-callback.
/// Replaced (and so dropped, safely outside any callback) by the next window of the same view.
struct CloseWatch<V> {
    _subscription: Subscription,
    _view: PhantomData<V>,
}

impl<V: 'static> Global for CloseWatch<V> {}

/// `name` only labels the log line of a failed open.
pub(crate) fn open_shell_window<V: ShellView>(
    st: Arc<AppState>,
    name: &'static str,
    default_size: Size<Pixels>,
    build: impl FnOnce(Arc<AppState>, &mut Window, &mut Context<V>) -> V,
    cx: &mut App,
) {
    let bounds = Bounds::centered(None, default_size, cx);
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
        let view = cx.new(|cx| build(st, window, cx));
        view_slot = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    });
    match (res, view_slot) {
        (Ok(window), Some(view)) => {
            cx.set_global(ShellWindow { window, view });
            watch_close::<V>(window.window_id(), cx);
        }
        (Err(e), _) => tracing::error!("could not open {name}: {e}"),
        _ => {}
    }
}

/// The global holds the view strongly, so closing the window alone would not release it. Both ids must match, the
/// closed window and the window of the handle currently installed, so neither a close of any other window nor an old
/// observer can touch a newer window's handle.
fn watch_close<V: ShellView>(id: WindowId, cx: &mut App) {
    let close = cx.on_window_closed(move |cx, closed| {
        if closed != id {
            return;
        }
        let Some(h) = cx.try_global::<ShellWindow<V>>() else { return };
        if h.window.window_id() != id {
            return;
        }
        let view = h.view.clone();
        cx.remove_global::<ShellWindow<V>>();
        view.update(cx, |view, cx| view.on_window_closed(cx));
    });
    cx.set_global(CloseWatch::<V> { _subscription: close, _view: PhantomData });
}
