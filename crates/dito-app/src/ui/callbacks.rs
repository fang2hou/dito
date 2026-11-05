use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

pub type Cb = Rc<dyn Fn(&mut Window, &mut App)>;
pub type IndexCb = Rc<dyn Fn(usize, &mut Window, &mut App)>;
pub type ToggleCb = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Callback that updates `V` when invoked from a plain closure (menus, segmented items, switches).
pub fn cbk<V: 'static>(cx: &mut Context<V>, f: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static) -> Cb {
    let weak = cx.entity().downgrade();
    Rc::new(move |window, app| {
        weak.update(app, |v, cx| f(v, window, cx)).ok();
    })
}

pub fn click_cb(cb: Cb) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, w, cx| cb(w, cx)
}

/// Segmented-control callback: gets the chosen index, the window and the owning view's context.
pub fn cbk_idx<V: 'static>(
    cx: &mut Context<V>,
    f: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static,
) -> IndexCb {
    let weak = cx.entity().downgrade();
    Rc::new(move |i, window, app| {
        weak.update(app, |v, cx| f(v, i, window, cx)).ok();
    })
}
