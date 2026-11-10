//! What a configuration row remembers between frames. The owner of the row holds one [`TileRowState`], like it holds
//! a `ScrollHandle`, and hands the same one to the row every time it draws.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::SharedString;

/// How far a press must move before it stops being a click.
const DRAG_THRESHOLD: f32 = 4.;

struct Press {
    id: SharedString,
    x: f32,
    y: f32,
}

/// What letting go of the pointer meant.
#[derive(Debug, PartialEq)]
pub(super) enum Outcome {
    /// Nothing was going on.
    Ignored,
    Click(SharedString),
}

/// The state of one configuration row. Cheap to clone: every clone is the same row.
#[derive(Clone, Default)]
pub struct TileRowState(Rc<RefCell<Option<Press>>>);

impl TileRowState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The pointer went down on tile `id` at window position (`x`, `y`).
    pub(super) fn press(&self, id: SharedString, x: f32, y: f32) {
        *self.0.borrow_mut() = Some(Press { id, x, y });
    }

    /// The pointer moved to window position (`x`, `y`): a press that went too far is not a click any more.
    pub(super) fn moved(&self, x: f32, y: f32, left_down: bool) {
        let mut press = self.0.borrow_mut();
        if press.as_ref().is_some_and(|p| !left_down || (x - p.x).hypot(y - p.y) >= DRAG_THRESHOLD) {
            *press = None;
        }
    }

    pub(super) fn released(&self) -> Outcome {
        match self.0.borrow_mut().take() {
            Some(p) => Outcome::Click(p.id),
            None => Outcome::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_that_barely_moves_is_a_click() {
        let state = TileRowState::new();
        state.press("b".into(), 100., 30.);
        state.moved(102., 31., true);
        assert_eq!(state.released(), Outcome::Click("b".into()));
        assert_eq!(state.released(), Outcome::Ignored, "one press, one click");
    }

    #[test]
    fn a_press_that_travels_is_not_a_click() {
        let state = TileRowState::new();
        state.press("b".into(), 100., 30.);
        state.moved(140., 30., true);
        assert_eq!(state.released(), Outcome::Ignored);
        state.press("b".into(), 100., 30.);
        state.moved(101., 30., false);
        assert_eq!(state.released(), Outcome::Ignored, "the button was let go where nothing listened");
    }
}
