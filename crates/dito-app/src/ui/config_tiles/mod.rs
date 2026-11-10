//! The row of configuration tiles above a cloud form (`20.05.13`).
//!
//! Up to three tiles share the width; from the fourth every tile is 200 wide and the row scrolls sideways, without a
//! scrollbar, fading into the card where tiles continue past an edge. The plus button sits outside the scrolling
//! area. A press that stays put selects.
//!
//! The row knows nothing about configurations: it takes plain tiles and reports ids. Its owner keeps one
//! [`TileRowState`] next to the data, passes the same one every frame and, after selecting or adding a tile,
//! calls [`TileRowState::reveal`].

mod geometry;
mod state;
mod tile;

use std::rc::Rc;
use std::time::Instant;

use gpui_kit::prelude::*;
use gpui_kit::*;

use self::geometry::{FADE_W, GAP, TILE_H};
use self::state::Outcome;
pub use self::state::TileRowState;
use self::tile::tile;
use super::button::{CONTROL_H, CircleAppearance, CircleSize, circle_control};
use super::callbacks::Cb;
use super::menu_button::{press_mark, press_toggle};
use super::motion::Motion;
use super::primitives::hex_alpha;
use super::scope::{ScopeHandle, motion_scope, with_motion_scope};
use crate::assets::Ic;
use crate::theme::Palette;

/// Called with the id of the tile that was clicked.
pub type IdCb = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// [`IdCb`] that updates the view `V`.
pub fn cbk_id<V: 'static>(
    cx: &mut Context<V>,
    f: impl Fn(&mut V, &str, &mut Window, &mut Context<V>) + 'static,
) -> IdCb {
    let weak = cx.entity().downgrade();
    Rc::new(move |id, window, app| {
        weak.update(app, |v, cx| f(v, id, window, cx)).ok();
    })
}

/// One tile: the name over what it is set up with, and whether it is the one in use.
pub struct ConfigTile {
    pub id: SharedString,
    pub name: SharedString,
    pub detail: SharedString,
    pub selected: bool,
}

/// The plus button that ends the row, outside the scrolling area.
pub struct TilePlus {
    /// Whether the menu the press opens is showing; the button stays lit and a press then closes it.
    pub open: bool,
    /// The press, to open or close the menu.
    pub on_press: Cb,
    /// The menu, drawn as a child of the button's own 36 pt box. Put it in [`float_menu`](super::float_menu) at
    /// `CONTROL_H + 8` to hang it under the button with the right edges aligned.
    pub menu: Option<AnyElement>,
}

/// The row of `tiles`, in order. `state` is the owner's, the same one every frame.
pub fn tile_row(id: impl Into<ElementId>, p: &Palette, state: &TileRowState, tiles: Vec<ConfigTile>) -> TileRow {
    TileRow { id: id.into(), p: *p, scope: motion_scope(), state: state.clone(), tiles, on_select: None, plus: None }
}

#[derive(IntoElement)]
pub struct TileRow {
    id: ElementId,
    p: Palette,
    /// The scope the row was built under: it draws after the page's scope guard has ended.
    scope: ScopeHandle,
    state: TileRowState,
    tiles: Vec<ConfigTile>,
    on_select: Option<IdCb>,
    plus: Option<TilePlus>,
}

impl TileRow {
    /// A click on a tile.
    pub fn on_select(mut self, on_select: IdCb) -> Self {
        self.on_select = Some(on_select);
        self
    }

    pub fn plus(mut self, plus: TilePlus) -> Self {
        self.plus = Some(plus);
        self
    }
}

impl RenderOnce for TileRow {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Self { id, p, scope, state, tiles, on_select, plus } = self;
        with_motion_scope(scope, move || {
            let ids: Vec<SharedString> = tiles.iter().map(|t| t.id.clone()).collect();
            let frame = state.frame(Instant::now(), &ids, Motion::system().is_reduced());
            if frame.animating {
                window.request_animation_frame();
            }
            let layout = frame.layout;

            let mut track = div().relative().left(px(-frame.offset)).h(px(TILE_H)).flex().gap(px(GAP));
            track = if layout.shares_width() { track.w_full() } else { track.w(px(layout.content())).flex_none() };
            for spec in &tiles {
                let press = {
                    let (state, tile_id) = (state.clone(), spec.id.clone());
                    move |e: &MouseDownEvent, _: &mut Window, _: &mut App| {
                        state.press(tile_id.clone(), f32::from(e.position.x), f32::from(e.position.y));
                    }
                };
                let one = tile(&p, ElementId::from((id.clone(), spec.id.clone())), spec, true)
                    .on_mouse_down(MouseButton::Left, press);
                track = track.child(if layout.shares_width() {
                    one.flex_1().min_w(px(0.))
                } else {
                    one.w(px(layout.w)).flex_none()
                });
            }

            let scroller = div()
                .relative()
                .flex_1()
                .min_w(px(0.))
                .h(px(TILE_H))
                .overflow_hidden()
                .on_scroll({
                    let state = state.clone();
                    move |e: &ScrollEvent, window, cx| {
                        let delta = e.delta.pixel_delta(window.line_height());
                        if state.wheel(f32::from(delta.x), f32::from(delta.y)) {
                            cx.stop_propagation();
                            window.refresh();
                        }
                    }
                })
                .child(gestures(&state, on_select).absolute().inset_0())
                .child(track)
                .when(frame.fades.0 > 0., |d| d.child(fade(&p, frame.fades.0, true)))
                .when(frame.fades.1 > 0., |d| d.child(fade(&p, frame.fades.1, false)));

            div()
                .relative()
                .w_full()
                .h(px(TILE_H))
                .flex()
                .items_center()
                .gap(px(GAP))
                .child(scroller)
                .children(plus.map(|plus| plus_button(&p, ElementId::from((id.clone(), "plus")), plus)))
        })
    }
}

/// The 36 pt circle at the end of the row, with the owner's menu hanging off it.
fn plus_button(p: &Palette, id: ElementId, plus: TilePlus) -> Div {
    let TilePlus { open, on_press, menu } = plus;
    let scope = motion_scope();
    let button = circle_control(
        id.clone(),
        Ic::Plus,
        CircleSize::Regular,
        CircleAppearance {
            background: Some(if open { p.seg } else { p.input_bg }),
            foreground: p.text,
            border: Some(p.border_strong),
            hover: Some(p.seg),
        },
    )
    .cursor_pointer()
    .on_mouse_down(MouseButton::Left, press_mark(scope.clone(), id.clone(), open))
    .on_click(press_toggle(scope, id, on_press));
    div().relative().flex_none().size(px(CONTROL_H)).child(button).children(menu)
}

/// An edge of the row fading into the card: opaque at the edge, clear `FADE_W` in.
fn fade(p: &Palette, strength: f32, left: bool) -> Div {
    let (solid, clear) = (hex_alpha(p.card, p.card.a * strength), hex_alpha(p.card, 0.));
    let strip = div().absolute().top_0().h(px(TILE_H)).w(px(FADE_W));
    if left {
        strip.left_0().bg(linear_gradient(90., linear_color_stop(solid, 0.), linear_color_stop(clear, 1.)))
    } else {
        strip.right_0().bg(linear_gradient(90., linear_color_stop(clear, 0.), linear_color_stop(solid, 1.)))
    }
}

/// Measures the scrolling area and, while it is painted, listens to the pointer wherever it goes: a press is only a
/// click if the pointer stays close to where it went down and is let go.
fn gestures(state: &TileRowState, on_select: Option<IdCb>) -> Canvas<()> {
    let measure = state.clone();
    let state = state.clone();
    canvas(
        move |bounds, window, _| {
            if measure.measure(f32::from(bounds.origin.x), f32::from(bounds.size.width)) {
                window.request_animation_frame();
            }
        },
        move |_, (), window, _| {
            let moves = state.clone();
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, _| {
                if phase == DispatchPhase::Capture {
                    let held = e.pressed_button == Some(MouseButton::Left);
                    moves.moved(f32::from(e.position.x), f32::from(e.position.y), held);
                }
            });
            window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || e.button != MouseButton::Left {
                    return;
                }
                if let Outcome::Click(id) = state.released(Instant::now())
                    && let Some(on_select) = &on_select
                {
                    on_select(&id, window, cx);
                    window.refresh();
                }
            });
        },
    )
}
