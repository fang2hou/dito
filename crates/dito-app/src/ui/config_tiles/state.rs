//! What a configuration row remembers between frames: how far it is scrolled and the press in progress. The owner
//! of the row holds one [`TileRowState`], like it holds a `ScrollHandle`; the row reads and advances it every time
//! it draws.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::SharedString;

use super::geometry::{Layout, wheel_delta};
use crate::ui::tween::ease;

/// Bringing a tile into view is a longer move than a hover (`motion/sidebar`).
const GLIDE_MS: u64 = 260;
/// How far a press must move before it stops being a click.
const DRAG_THRESHOLD: f32 = 4.;
/// How long a request to reveal a tile waits for that tile to exist.
const REVEAL_PATIENCE: Duration = Duration::from_secs(1);

/// A value that eases from one number to another.
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    ms: u64,
}

impl Tween {
    fn value(&self, now: Instant) -> f32 {
        self.from + (self.to - self.from) * ease(self.progress(now))
    }

    fn running(&self, now: Instant) -> bool {
        self.progress(now) < 1.
    }

    fn progress(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.start).as_secs_f32() / (self.ms as f32 / 1000.)).clamp(0., 1.)
    }
}

struct Press {
    id: SharedString,
    x: f32,
    y: f32,
}

struct Reveal {
    id: SharedString,
    at: Instant,
}

struct Inner {
    layout: Layout,
    /// Left edge and width of the scrolling area in window coordinates, once it has been painted.
    view: Option<(f32, f32)>,
    offset: f32,
    glide: Option<Tween>,
    reveal: Option<Reveal>,
    press: Option<Press>,
    /// Whether a frame with a measured row has been drawn: what is asked before that appears in place.
    shown: bool,
}

/// What the row draws for one frame.
pub(super) struct Frame {
    pub layout: Layout,
    pub offset: f32,
    /// Strength of the fade over the left and right edge.
    pub fades: (f32, f32),
    /// Whether anything is still moving, so the next frame has to be asked for.
    pub animating: bool,
}

/// What letting go of the pointer meant.
#[derive(Debug, PartialEq)]
pub(super) enum Outcome {
    /// Nothing was going on.
    Ignored,
    Click(SharedString),
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
            layout: Layout::new(0., 0),
            view: None,
            offset: 0.,
            glide: None,
            reveal: None,
            press: None,
            shown: false,
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

    /// The pointer went down on tile `id` at window position (`x`, `y`).
    pub(super) fn press(&self, id: SharedString, x: f32, y: f32) {
        self.0.borrow_mut().press = Some(Press { id, x, y });
    }

    /// The pointer moved to window position (`x`, `y`): a press that went too far is not a click any more.
    pub(super) fn moved(&self, x: f32, y: f32, left_down: bool) {
        let mut s = self.0.borrow_mut();
        if s.press.as_ref().is_some_and(|p| !left_down || (x - p.x).hypot(y - p.y) >= DRAG_THRESHOLD) {
            s.press = None;
        }
    }

    /// The pointer was let go. A click selects the tile, which the row then brings fully into view.
    pub(super) fn released(&self, now: Instant) -> Outcome {
        let mut s = self.0.borrow_mut();
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
        if max <= 0. {
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

    /// For the screenshot harness: the scroll offset and the furthest it goes.
    #[cfg(feature = "screenshots")]
    pub fn probe(&self) -> (f32, f32) {
        let s = self.0.borrow();
        (s.offset, s.layout.max_offset())
    }
}

impl Inner {
    fn frame(&mut self, now: Instant, ids: &[SharedString], reduced: bool) -> Frame {
        let layout = Layout::new(self.view.map_or(0., |(_, w)| w), ids.len());
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
        self.offset = self.offset.clamp(0., layout.max_offset());
        self.shown |= layout.view > 0.;
        Frame { layout, offset: self.offset, fades: layout.fades(self.offset), animating: self.glide.is_some() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: f32 = 100.;

    fn ids(n: usize) -> Vec<SharedString> {
        ["a", "b", "c", "d", "e"][..n].iter().map(|s| SharedString::from(*s)).collect()
    }

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// A row of five tiles drawn once at `LEFT`, 672 wide.
    fn row() -> (TileRowState, Instant) {
        let state = TileRowState::new();
        let t0 = Instant::now();
        state.measure(LEFT, 672.);
        state.frame(t0, &ids(5), false);
        (state, t0)
    }

    #[test]
    fn a_press_that_barely_moves_is_a_click() {
        let state = TileRowState::new();
        let t0 = Instant::now();
        state.press("b".into(), 100., 30.);
        state.moved(102., 31., true);
        assert_eq!(state.released(t0), Outcome::Click("b".into()));
        assert_eq!(state.released(t0), Outcome::Ignored, "one press, one click");
    }

    #[test]
    fn a_press_that_travels_is_not_a_click() {
        let state = TileRowState::new();
        let t0 = Instant::now();
        state.press("b".into(), 100., 30.);
        state.moved(140., 30., true);
        assert_eq!(state.released(t0), Outcome::Ignored);
        state.press("b".into(), 100., 30.);
        state.moved(101., 30., false);
        assert_eq!(state.released(t0), Outcome::Ignored, "the button was let go where nothing listened");
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
    fn a_click_on_a_tile_that_is_only_half_there_scrolls_it_into_view() {
        let (state, t0) = row();
        state.press("d".into(), 100., 30.);
        assert_eq!(state.released(at(t0, 10)), Outcome::Click("d".into()));
        state.frame(at(t0, 16), &ids(5), false);
        let end = state.frame(at(t0, 16 + 400), &ids(5), false);
        assert_eq!(end.offset, 212.);
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
