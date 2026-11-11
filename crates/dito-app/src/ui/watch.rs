use gpui_kit::*;
use tokio::sync::watch;

pub fn notify_on_change<V: 'static, T: 'static>(mut rx: watch::Receiver<T>, cx: &mut Context<V>) {
    cx.spawn(async move |this, cx| {
        while rx.changed().await.is_ok() {
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// The view is not re-rendered unless `on_change` notifies.
pub fn observe<V: 'static, T: Clone + 'static>(
    mut rx: watch::Receiver<T>,
    cx: &mut Context<V>,
    mut on_change: impl FnMut(&mut V, T, &mut Context<V>) + 'static,
) {
    cx.spawn(async move |this, cx| {
        while rx.changed().await.is_ok() {
            let value = rx.borrow_and_update().clone();
            if this.update(cx, |view, cx| on_change(view, value, cx)).is_err() {
                break;
            }
        }
    })
    .detach();
}
