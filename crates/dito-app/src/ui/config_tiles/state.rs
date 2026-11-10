//! What a configuration row remembers between frames: how far it is scrolled, the press or drag in progress and the
//! motion of the tiles that slide aside. The owner of the row holds one [`TileRowState`], like it holds a
//! `ScrollHandle`; the row reads and advances it every time it draws.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::SharedString;

use super::geometry::{Layout, edge_speed, slots_moved, wheel_delta};
use crate::ui::tween::{RING_MS, ease};

/// Tiles sliding aside, the lift and the drop settle on the focus-ring clock (`motion/ring`).
const SLIDE_MS: u64 = RING_MS;
/// Bringing a tile into view is a longer move (`motion/sidebar`).
const GLIDE_MS: u64 = 260;
/// How far a press must move before it lifts the tile instead of being a click.
const DRAG_THRESHOLD: f32 = 4.;
/// A frame after a long pause advances at most this far, so a stall never throws the row across.
const MAX_DT: f32 = 0.05;
/// How long a request to reveal a tile waits for that tile to exist.
const REVEAL_PATIENCE: Duration = Duration::from_secs(1);

/// A value that eases from one number to another; asking for a new target mid-flight continues from where it is.
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    ms: u64,
}

impl Tween {
    fn value(&self, now: Instant) -> f32 {
        self.from + (self.to - self.from) * ease(progress(self.start, now, self.ms))
    }

    fn running(&self, now: Instant) -> bool {
        progress(self.start, now, self.ms) < 1.
    }

    fn retarget(&mut self, now: Instant, to: f32, ms: u64) {
        if (self.to - to).abs() > f32::EPSILON {
            *self = Self { from: self.value(now), to, start: now, ms };
        }
    }
}

/// Fraction of a `ms` long motion that started at `start` and has run until `now`; a motion of no length is done.
fn progress(start: Instant, now: Instant, ms: u64) -> f32 {
    if ms == 0 {
        return 1.;
    }
    (now.saturating_duration_since(start).as_secs_f32() / (ms as f32 / 1000.)).clamp(0., 1.)
}

struct Press {
    id: SharedString,
    index: usize,
    x: f32,
    y: f32,
    /// How far into the tile the pointer went down; unknown until the row has been drawn once.
    grab: Option<f32>,
}

struct Drag {
    id: SharedString,
    from: usize,
    grab: f32,
    /// The pointer, from the left edge of the scrolling area.
    pointer: f32,
    /// The lifted tile's left edge, from the left edge of the scrolling area.
    left: f32,
    landing: usize,
    since: Instant,
}

/// A dropped tile settling into its slot.
struct Settle {
    id: SharedString,
    from: f32,
    lift: f32,
    start: Instant,
    ms: u64,
}

struct Reveal {
    id: SharedString,
    at: Instant,
}

struct Inner {
    /// The tile ids and their geometry as of the last frame.
    ids: Vec<SharedString>,
    layout: Layout,
    /// Left edge and width of the scrolling area in window coordinates, once it has been painted.
    view: Option<(f32, f32)>,
    offset: f32,
    glide: Option<Tween>,
    reveal: Option<Reveal>,
    press: Option<Press>,
    drag: Option<Drag>,
    settle: Option<Settle>,
    /// How far each tile is displaced from its place in the row, by id.
    slides: HashMap<SharedString, Tween>,
    tick: Option<Instant>,
    /// Whether a frame with a measured row has been drawn: what is asked before that appears in place.
    shown: bool,
    reduced: bool,
}

/// What the row draws for one frame.
pub(super) struct Frame {
    pub layout: Layout,
    pub offset: f32,
    /// How far each tile is displaced from its place, in points.
    pub shifts: Vec<f32>,
    /// The tile that is drawn as the floating one (or as it settles) instead of in the row.
    pub hidden: Option<usize>,
    pub overlay: Option<Overlay>,
    /// Strength of the fade over the left and right edge.
    pub fades: (f32, f32),
    /// Whether anything is still moving, so the next frame has to be asked for.
    pub animating: bool,
}

/// The floating tile: its left edge from the left edge of the scrolling area and how far it is lifted, 0 to 1.
pub(super) struct Overlay {
    pub index: usize,
    pub left: f32,
    pub lift: f32,
}

/// What letting go of the pointer meant.
#[derive(Debug, PartialEq)]
pub(super) enum Outcome {
    /// Nothing was going on.
    Ignored,
    /// A drag ended where it started.
    Back,
    Click(SharedString),
    Drop {
        id: SharedString,
        to: usize,
    },
}

/// The state of one configuration row. Cheap to clone: every clone is the same row.
#[derive(Clone)]
pub struct TileRowState(Rc<RefCell<Inner>>);

impl Default for TileRowState {
    fn default() -> Self {
        Self::new()
    }
}

impl TileRowState {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(Inner {
            ids: Vec::new(),
            layout: Layout::new(0., 0),
            view: None,
            offset: 0.,
            glide: None,
            reveal: None,
            press: None,
            drag: None,
            settle: None,
            slides: HashMap::new(),
            tick: None,
            shown: false,
            reduced: false,
        })))
    }

    /// Scroll tile `id` fully into view the next time the row draws (after selecting or adding it). It waits for the
    /// row to be on screen and for the tile to exist; a row that is not on screen yet shows it in place.
    pub fn reveal(&self, id: impl Into<SharedString>) {
        self.0.borrow_mut().reveal = Some(Reveal { id: id.into(), at: Instant::now() });
    }

    /// Remember where the scrolling area was painted. `true` when its width changed.
    pub(super) fn measure(&self, left: f32, width: f32) -> bool {
        let mut s = self.0.borrow_mut();
        let widened = s.view.is_none_or(|(_, w)| (w - width).abs() > 0.5);
        s.view = Some((left, width));
        widened
    }

    pub(super) fn frame(&self, now: Instant, ids: &[SharedString], reduced: bool) -> Frame {
        self.0.borrow_mut().frame(now, ids, reduced)
    }

    pub(super) fn dragging(&self) -> bool {
        self.0.borrow().drag.is_some()
    }

    /// The pointer went down on tile `index` at window position (`x`, `y`).
    pub(super) fn press(&self, index: usize, id: SharedString, x: f32, y: f32) {
        let mut s = self.0.borrow_mut();
        let grab = s.view.map(|(left, _)| (x - left) - (s.layout.left(index) - s.offset));
        s.press = Some(Press { id, index, x, y, grab });
        s.settle = None;
    }

    /// The pointer moved to window position (`x`, `y`). `true` when the row has to draw again.
    pub(super) fn moved(&self, x: f32, y: f32, left_down: bool, now: Instant) -> bool {
        let mut s = self.0.borrow_mut();
        if s.press.is_none() && s.drag.is_none() {
            return false;
        }
        if !left_down {
            // The release happened where nothing was listening: end the gesture without a click or a move.
            s.press = None;
            s.end_drag(x, now);
            return true;
        }
        let Some((view_left, _)) = s.view else { return false };
        if let Some(d) = &mut s.drag {
            d.pointer = x - view_left;
            return true;
        }
        let Some(p) = &s.press else { return false };
        let travelled = (x - p.x).hypot(y - p.y);
        let (Some(grab), true) = (p.grab, s.layout.n > 1) else { return false };
        if travelled < DRAG_THRESHOLD {
            return false;
        }
        let (id, from) = (p.id.clone(), p.index);
        s.press = None;
        s.glide = None;
        s.drag = Some(Drag { id, from, grab, pointer: x - view_left, left: 0., landing: from, since: now });
        true
    }

    /// The pointer was let go at window x position `x`.
    pub(super) fn released(&self, x: f32, now: Instant) -> Outcome {
        let mut s = self.0.borrow_mut();
        if let Some(drop) = s.end_drag(x, now) {
            return drop;
        }
        match s.press.take() {
            Some(p) => {
                s.reveal = Some(Reveal { id: p.id.clone(), at: now });
                Outcome::Click(p.id)
            }
            None => Outcome::Ignored,
        }
    }

    /// One scroll event; `true` when the row takes it, which then keeps it from scrolling whatever is behind it.
    /// A vertical wheel is the row's while it can still move that way and passes on to the page at either end; a
    /// sideways swipe is always the row's, since nothing behind it scrolls sideways.
    pub(super) fn wheel(&self, dx: f32, dy: f32) -> bool {
        let mut s = self.0.borrow_mut();
        let max = s.layout.max_offset();
        if s.drag.is_some() || max <= 0. {
            return false;
        }
        let next = (s.offset - wheel_delta(dx, dy)).clamp(0., max);
        let moved = (next - s.offset).abs() >= 0.01;
        if moved {
            s.offset = next;
            s.glide = None;
            s.reveal = None;
        }
        moved || dx.abs() > dy.abs()
    }

    /// For the screenshot harness: the scroll offset, the furthest it goes and the slot a dragged tile is over.
    #[cfg(feature = "screenshots")]
    pub fn probe(&self) -> (f32, f32, Option<usize>) {
        let s = self.0.borrow();
        (s.offset, s.layout.max_offset(), s.drag.as_ref().map(|d| d.landing))
    }
}

impl Inner {
    /// Settle a drag at window x position `x`; the drop it amounts to, if it moved the tile.
    fn end_drag(&mut self, x: f32, now: Instant) -> Option<Outcome> {
        let d = self.drag.take()?;
        let view_left = self.view.map_or(0., |(left, _)| left);
        let left = self.lifted_left(x - view_left - d.grab);
        let to = self.layout.landing(left, self.offset);
        let ms = if self.reduced { 0 } else { SLIDE_MS };
        let lift = ease(progress(d.since, now, ms));
        self.settle = Some(Settle { id: d.id.clone(), from: left, lift, start: now, ms });
        Some(if to == d.from { Outcome::Back } else { Outcome::Drop { id: d.id, to } })
    }

    /// Keep the lifted tile inside the scrolling area.
    fn lifted_left(&self, left: f32) -> f32 {
        left.clamp(0., (self.layout.view - self.layout.w).max(0.))
    }

    fn frame(&mut self, now: Instant, ids: &[SharedString], reduced: bool) -> Frame {
        self.reduced = reduced;
        let ms = if reduced { 0 } else { SLIDE_MS };
        let dt = self.tick.map_or(0., |t| now.saturating_duration_since(t).as_secs_f32().min(MAX_DT));
        self.tick = Some(now);

        let layout = Layout::new(self.view.map_or(0., |(_, w)| w), ids.len());
        if ids != self.ids.as_slice() {
            self.reorder(now, ids, &layout, ms);
        }
        self.layout = layout;

        if let Some(reveal) = &self.reveal {
            let index = ids.iter().position(|id| *id == reveal.id).filter(|_| layout.view > 0.);
            if let Some(index) = index {
                let target = layout.reveal(index, self.offset);
                self.reveal = None;
                if (target - self.offset).abs() > 0.5 && self.shown && !reduced {
                    self.glide = Some(Tween { from: self.offset, to: target, start: now, ms: GLIDE_MS });
                } else {
                    self.offset = target;
                    self.glide = None;
                }
            } else if now.saturating_duration_since(reveal.at) > REVEAL_PATIENCE {
                self.reveal = None;
            }
        }
        if let Some(glide) = self.glide {
            self.offset = glide.value(now);
            if !glide.running(now) {
                self.glide = None;
            }
        }

        let mut scrolling = false;
        if let Some(d) = &mut self.drag {
            let speed = edge_speed(d.pointer, layout.view);
            if speed != 0. && layout.max_offset() > 0. {
                self.offset += speed * dt;
                self.glide = None;
                scrolling = true;
            }
        }
        self.offset = self.offset.clamp(0., layout.max_offset());
        if let Some(d) = &mut self.drag {
            d.left = d.pointer - d.grab;
            d.left = d.left.clamp(0., (layout.view - layout.w).max(0.));
            d.landing = layout.landing(d.left, self.offset);
        }

        let mut shifts = Vec::with_capacity(ids.len());
        for (index, id) in ids.iter().enumerate() {
            let target = match &self.drag {
                Some(d) if index != d.from => slots_moved(index, d.from, d.landing) * layout.pitch,
                _ => 0.,
            };
            if target != 0. {
                let at_rest = Tween { from: 0., to: 0., start: now, ms };
                self.slides.entry(id.clone()).or_insert(at_rest).retarget(now, target, ms);
            } else if let Some(tween) = self.slides.get_mut(id) {
                tween.retarget(now, 0., ms);
            }
            shifts.push(self.slides.get(id).map_or(0., |tween| tween.value(now)));
        }
        self.slides.retain(|id, t| ids.contains(id) && (t.to != 0. || t.running(now)));

        let (overlay, hidden) = self.floating(now, ids, &layout);
        let animating = scrolling
            || self.glide.is_some()
            || self.settle.is_some()
            || self.drag.as_ref().is_some_and(|d| progress(d.since, now, ms) < 1.)
            || self.slides.values().any(|t| t.running(now));
        self.shown |= layout.view > 0.;
        Frame { layout, offset: self.offset, shifts, hidden, overlay, fades: layout.fades(self.offset), animating }
    }

    /// The tile that floats above the row: the dragged one, or the dropped one on its way into its slot.
    fn floating(&mut self, now: Instant, ids: &[SharedString], layout: &Layout) -> (Option<Overlay>, Option<usize>) {
        if let Some(d) = &self.drag {
            let ms = if self.reduced { 0 } else { SLIDE_MS };
            let lift = ease(progress(d.since, now, ms));
            return (Some(Overlay { index: d.from, left: d.left, lift }), Some(d.from));
        }
        let Some(s) = &self.settle else { return (None, None) };
        let index = ids.iter().position(|id| *id == s.id);
        let t = progress(s.start, now, s.ms);
        let Some(index) = index.filter(|_| t < 1.) else {
            self.settle = None;
            return (None, None);
        };
        let e = ease(t);
        let to = layout.left(index) - self.offset;
        (Some(Overlay { index, left: s.from + (to - s.from) * e, lift: s.lift * (1. - e) }), Some(index))
    }

    /// The tiles changed order, number or identity. Every tile that was already there keeps the place it was drawn at
    /// and slides from it to its new one, so a drop, an addition or a removal never makes the row jump.
    fn reorder(&mut self, now: Instant, ids: &[SharedString], layout: &Layout, ms: u64) {
        let old = self.layout;
        let mut slides = HashMap::new();
        if (old.w - layout.w).abs() < 0.5 {
            for (now_index, id) in ids.iter().enumerate() {
                let floating = self.drag.as_ref().is_some_and(|d| d.id == *id)
                    || self.settle.as_ref().is_some_and(|s| s.id == *id);
                let Some(was) = self.ids.iter().position(|x| x == id).filter(|_| !floating) else { continue };
                let drawn = old.left(was) + self.slides.get(id).map_or(0., |t| t.value(now));
                let shift = drawn - layout.left(now_index);
                if shift.abs() > 0.5 {
                    slides.insert(id.clone(), Tween { from: shift, to: 0., start: now, ms });
                }
            }
        }
        self.slides = slides;

        let position = |id: &SharedString| ids.iter().position(|x| x == id);
        if let Some(p) = &mut self.press {
            match position(&p.id) {
                Some(index) => p.index = index,
                None => self.press = None,
            }
        }
        if let Some(d) = &mut self.drag {
            match position(&d.id) {
                Some(index) => d.from = index,
                None => self.drag = None,
            }
        }
        if self.settle.as_ref().is_some_and(|s| position(&s.id).is_none()) {
            self.settle = None;
        }
        self.ids = ids.to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: f32 = 100.;

    fn ids(n: usize) -> Vec<SharedString> {
        ["a", "b", "c", "d", "e"][..n].iter().map(|s| SharedString::from(*s)).collect()
    }

    /// A row of five tiles drawn once at `LEFT`, 672 wide.
    fn row() -> (TileRowState, Instant) {
        let state = TileRowState::new();
        let t0 = Instant::now();
        state.measure(LEFT, 672.);
        state.frame(t0, &ids(5), false);
        (state, t0)
    }

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// Window x of the pointer that holds tile `index` (unscrolled) `grab` from its left edge, with the tile `left` from
    /// the left edge of the row.
    fn pointer(left: f32, grab: f32) -> f32 {
        LEFT + left + grab
    }

    #[test]
    fn a_press_that_barely_moves_is_a_click() {
        let (state, t0) = row();
        let x = pointer(212., 50.);
        state.press(1, "b".into(), x, 30.);
        assert!(!state.moved(x + 2., 31., true, at(t0, 10)), "inside the threshold nothing lifts");
        assert_eq!(state.released(x + 2., at(t0, 20)), Outcome::Click("b".into()));
        assert!(!state.dragging());
    }

    #[test]
    fn a_drag_reports_the_slot_it_was_dropped_in() {
        let (state, t0) = row();
        state.press(1, "b".into(), pointer(212., 50.), 30.);
        assert!(state.moved(pointer(300., 50.), 30., true, at(t0, 10)), "past the threshold the tile lifts");
        // The design's drag specimen: the tile 446 from the left edge of the row, so its centre is over slot 2.
        let x = pointer(446., 50.);
        assert!(state.moved(x, 30., true, at(t0, 20)));
        let frame = state.frame(at(t0, 30), &ids(5), false);
        assert_eq!(frame.hidden, Some(1), "the dragged tile floats instead of sitting in the row");
        assert_eq!(frame.overlay.map(|o| o.left), Some(446.));
        assert_eq!(state.released(x, at(t0, 40)), Outcome::Drop { id: "b".into(), to: 2 });
    }

    #[test]
    fn letting_go_over_the_old_slot_moves_nothing() {
        let (state, t0) = row();
        state.press(1, "b".into(), pointer(212., 50.), 30.);
        state.moved(pointer(230., 50.), 30., true, at(t0, 10));
        state.frame(at(t0, 20), &ids(5), false);
        assert_eq!(state.released(pointer(230., 50.), at(t0, 30)), Outcome::Back);
    }

    #[test]
    fn the_tiles_in_between_slide_aside_and_stay_where_they_are_after_the_drop() {
        let (state, t0) = row();
        state.press(1, "b".into(), pointer(212., 50.), 30.);
        let x = pointer(446., 50.);
        state.moved(x, 30., true, at(t0, 10));
        state.frame(at(t0, 20), &ids(5), false);
        // Long enough for the slide to finish: tile c (index 2) made room by moving one pitch left.
        let frame = state.frame(at(t0, 400), &ids(5), false);
        assert_eq!(frame.shifts, [0., 0., -212., 0., 0.]);
        assert_eq!(state.released(x, at(t0, 410)), Outcome::Drop { id: "b".into(), to: 2 });
        // The owner applies the move; the tiles keep the places they were drawn at, so nothing jumps.
        let order: Vec<SharedString> = ["a", "c", "b", "d", "e"].iter().map(|s| SharedString::from(*s)).collect();
        let frame = state.frame(at(t0, 416), &order, false);
        assert!(frame.shifts.iter().all(|s| s.abs() < 0.5), "{:?}", frame.shifts);
        assert_eq!(frame.hidden, Some(2), "the dropped tile settles into its new slot");
        let frame = state.frame(at(t0, 410 + 400), &order, false);
        assert!(frame.overlay.is_none() && frame.hidden.is_none() && !frame.animating, "the drop has settled");
    }

    #[test]
    fn a_dragged_tile_held_near_the_edge_scrolls_the_row() {
        let (state, t0) = row();
        state.press(0, "a".into(), pointer(0., 50.), 30.);
        let x = pointer(672. - 40., 50.);
        state.moved(x, 30., true, at(t0, 10));
        let a = state.frame(at(t0, 16), &ids(5), false);
        let b = state.frame(at(t0, 32), &ids(5), false);
        assert!(a.animating && b.offset > a.offset, "it keeps scrolling while held there");
        let mut end = b;
        for n in 3..200 {
            end = state.frame(at(t0, 16 * n), &ids(5), false);
        }
        assert_eq!(end.offset, 376., "it stops at the end");
    }

    #[test]
    fn letting_go_where_nothing_listened_ends_the_gesture_quietly() {
        let (state, t0) = row();
        state.press(1, "b".into(), pointer(212., 50.), 30.);
        state.moved(pointer(300., 50.), 30., true, at(t0, 10));
        assert!(state.moved(pointer(300., 50.), 30., false, at(t0, 20)), "the next move without the button ends it");
        assert!(!state.dragging());
        assert_eq!(state.released(pointer(300., 50.), at(t0, 30)), Outcome::Ignored);
    }

    #[test]
    fn a_reveal_waits_for_the_row_to_be_drawn_and_then_shows_the_tile_in_place() {
        let state = TileRowState::new();
        let t0 = Instant::now();
        state.reveal("e");
        assert_eq!(state.frame(t0, &ids(5), false).offset, 0., "not measured yet: nothing to scroll");
        state.measure(LEFT, 672.);
        let frame = state.frame(at(t0, 16), &ids(5), false);
        assert_eq!(frame.offset, 376., "the first reveal needs no animation");
        assert!(!frame.animating);
    }

    #[test]
    fn a_reveal_on_a_row_that_is_on_screen_glides() {
        let (state, t0) = row();
        state.reveal("e");
        let start = state.frame(at(t0, 16), &ids(5), false);
        assert!(start.animating && start.offset < 376.);
        let end = state.frame(at(t0, 16 + 400), &ids(5), false);
        assert_eq!((end.offset, end.animating), (376., false));
    }

    #[test]
    fn the_wheel_scrolls_the_row_until_its_ends_and_then_lets_the_page_have_it() {
        let (state, _) = row();
        assert!(!state.wheel(0., 30.), "already at the start, scrolling back is not the row's business");
        assert!(state.wheel(0., -100.));
        assert!(state.wheel(0., -1000.), "to the end");
        assert!(!state.wheel(0., -10.), "nothing left to scroll");
        assert!(
            state.wheel(-10., 0.),
            "a sideways swipe at the end stays with the row: nothing behind it scrolls that way"
        );
        let few = TileRowState::new();
        few.measure(LEFT, 672.);
        few.frame(Instant::now(), &ids(3), false);
        assert!(!few.wheel(0., -100.), "a row that fits does not scroll");
    }
}
