//! The row of configuration tiles above a cloud form (`design:settings/cloud-models/tile-row`).
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

use self::geometry::{FADE_W, GAP, LIFT, TILE_H, TILE_W};
use self::state::Outcome;
pub use self::state::TileRowState;
use self::tile::tile;
use super::button::{CONTROL_H, CircleAppearance, CircleSize, circle_control};
use super::callbacks::Cb;
use super::menu_button::{press_mark, press_toggle};
use super::motion::Motion;
use super::primitives::hex_alpha;
use super::scope::{ScopeHandle, motion_scope, with_motion_scope};
use crate::Ic;
use crate::Palette;

pub type IdCb = Rc<dyn Fn(&str, &mut Window, &mut App)>;
/// Gets the id of the dropped tile and the index it now has.
pub type MoveCb = Rc<dyn Fn(&str, usize, &mut Window, &mut App)>;

pub fn cbk_id<V: 'static>(
    cx: &mut Context<V>,
    f: impl Fn(&mut V, &str, &mut Window, &mut Context<V>) + 'static,
) -> IdCb {
    let weak = cx.entity().downgrade();
    Rc::new(move |id, window, app| {
        weak.update(app, |v, cx| f(v, id, window, cx)).ok();
    })
}

pub fn cbk_move<V: 'static>(
    cx: &mut Context<V>,
    f: impl Fn(&mut V, &str, usize, &mut Window, &mut Context<V>) + 'static,
) -> MoveCb {
    let weak = cx.entity().downgrade();
    Rc::new(move |id, to, window, app| {
        weak.update(app, |v, cx| f(v, id, to, window, cx)).ok();
    })
}

pub struct ConfigTile {
    pub id: SharedString,
    pub name: SharedString,
    pub detail: SharedString,
    pub selected: bool,
}

pub struct TilePlus {
    /// While the menu is showing the button stays lit, and a press closes it.
    pub open: bool,
    pub on_press: Cb,
    /// Drawn as a child of the button's own 36 pt box: pass `add_menu(...)`.
    pub menu: Option<AnyElement>,
}

pub fn tile_row(id: impl Into<ElementId>, p: &Palette, state: &TileRowState, tiles: Vec<ConfigTile>) -> TileRow {
    TileRow {
        id: id.into(),
        p: *p,
        scope: motion_scope(),
        state: state.clone(),
        tiles,
        on_select: None,
        on_move: None,
        plus: None,
    }
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
    on_move: Option<MoveCb>,
    plus: Option<TilePlus>,
}

impl TileRow {
    pub fn on_select(mut self, on_select: IdCb) -> Self {
        self.on_select = Some(on_select);
        self
    }

    /// The owner reorders its data and draws again.
    pub fn on_move(mut self, on_move: MoveCb) -> Self {
        self.on_move = Some(on_move);
        self
    }

    pub fn plus(mut self, plus: TilePlus) -> Self {
        self.plus = Some(plus);
        self
    }
}

impl RenderOnce for TileRow {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Self { id, p, scope, state, tiles, on_select, on_move, plus } = self;
        with_motion_scope(scope, move || {
            let ids: Vec<SharedString> = tiles.iter().map(|t| t.id.clone()).collect();
            let frame = state.frame(Instant::now(), &ids, Motion::system().is_reduced());
            if frame.animating {
                window.request_animation_frame();
            }
            let layout = frame.layout;
            let dragging = state.dragging() || frame.overlay.is_some();

            let mut track = div()
                .relative()
                .left(px(-frame.offset))
                .w(px(layout.content()))
                .flex_none()
                .h(px(TILE_H))
                .flex()
                .gap(px(GAP));
            for (index, spec) in tiles.iter().enumerate() {
                let press = {
                    let (state, tile_id) = (state.clone(), spec.id.clone());
                    move |e: &MouseDownEvent, _: &mut Window, _: &mut App| {
                        state.press(index, tile_id.clone(), f32::from(e.position.x), f32::from(e.position.y));
                    }
                };
                let mut one = tile(&p, ElementId::from((id.clone(), spec.id.clone())), spec, !dragging)
                    .left(px(frame.shifts[index]))
                    .w(px(TILE_W))
                    .flex_none()
                    .on_mouse_down(MouseButton::Left, press);
                if frame.hidden == Some(index) {
                    one = one.opacity(0.);
                }
                track = track.child(one);
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
                .child(gestures(&state, on_select, on_move).absolute().inset_0())
                .child(track)
                .when(frame.fades.0 > 0., |d| d.child(fade(&p, frame.fades.0, true)))
                .when(frame.fades.1 > 0., |d| d.child(fade(&p, frame.fades.1, false)));

            let overlay = frame.overlay.as_ref().map(|o| {
                let lifted = ElementId::from((ElementId::from((id.clone(), tiles[o.index].id.clone())), "lifted"));
                let shadow = BoxShadow {
                    color: hex_alpha(p.shadow, p.shadow.a * o.lift),
                    offset: point(px(0.), px(10.)),
                    blur_radius: px(28.),
                    spread_radius: px(0.),
                    inset: false,
                };
                deferred(
                    tile(&p, lifted, &tiles[o.index], false)
                        .absolute()
                        .left(px(o.left))
                        .top(px(-LIFT * o.lift))
                        .w(px(TILE_W))
                        .shadow(vec![shadow]),
                )
            });

            div()
                .relative()
                .w_full()
                .h(px(TILE_H))
                .flex()
                .items_center()
                .gap(px(GAP))
                .child(scroller)
                .children(plus.map(|plus| plus_button(&p, ElementId::from((id.clone(), "plus")), plus)))
                .children(overlay)
        })
    }
}

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

fn fade(p: &Palette, strength: f32, left: bool) -> Div {
    let (solid, clear) = (hex_alpha(p.card, p.card.a * strength), hex_alpha(p.card, 0.));
    let strip = div().absolute().top_0().h(px(TILE_H)).w(px(FADE_W));
    if left {
        strip.left_0().bg(linear_gradient(90., linear_color_stop(solid, 0.), linear_color_stop(clear, 1.)))
    } else {
        strip.right_0().bg(linear_gradient(90., linear_color_stop(clear, 0.), linear_color_stop(solid, 1.)))
    }
}

/// Listens to the pointer wherever it goes, because a drag keeps going when the pointer leaves the row.
fn gestures(state: &TileRowState, on_select: Option<IdCb>, on_move: Option<MoveCb>) -> Canvas<()> {
    let measure = state.clone();
    let state = state.clone();
    canvas(
        move |bounds, window, _| {
            if measure.measure(f32::from(bounds.origin.x), f32::from(bounds.size.width)) {
                window.request_animation_frame();
            }
        },
        move |_, (), window, _| {
            if state.dragging() {
                window.set_window_cursor_style(CursorStyle::ClosedHand);
            }
            let moves = state.clone();
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, _| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let held = e.pressed_button == Some(MouseButton::Left);
                if moves.moved(f32::from(e.position.x), f32::from(e.position.y), held, Instant::now()) {
                    window.refresh();
                }
            });
            window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || e.button != MouseButton::Left {
                    return;
                }
                match state.released(f32::from(e.position.x), Instant::now()) {
                    Outcome::Ignored => return,
                    Outcome::Back => {}
                    Outcome::Click(id) => {
                        if let Some(on_select) = &on_select {
                            on_select(&id, window, cx);
                        }
                    }
                    Outcome::Drop { id, to } => {
                        if let Some(on_move) = &on_move {
                            on_move(&id, to, window, cx);
                        }
                    }
                }
                window.refresh();
            });
        },
    )
}
