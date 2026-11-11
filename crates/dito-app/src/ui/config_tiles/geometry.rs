//! Plain arithmetic on points for the tile row; the numbers are the design's (`design:settings/cloud-models/tile-row`).

pub(super) const TILE_H: f32 = 64.;
pub(super) const TILE_W: f32 = 216.;
pub(super) const GAP: f32 = 12.;
pub(super) const FADE_W: f32 = 48.;
pub(super) const LIFT: f32 = 8.;
pub(super) const PITCH: f32 = TILE_W + GAP;
/// A fade grows over this distance, so reaching an end of the row never makes it pop out.
const FADE_RAMP: f32 = 24.;
/// Distance from a row edge inside which a dragged tile scrolls the row, and the speed right at the edge (pt/s).
const EDGE_ZONE: f32 = 48.;
const EDGE_SPEED: f32 = 600.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub view: f32,
    pub n: usize,
}

impl Layout {
    pub fn new(view: f32, n: usize) -> Self {
        Self { view, n }
    }

    pub fn left(&self, index: usize) -> f32 {
        index as f32 * PITCH
    }

    pub fn content(&self) -> f32 {
        if self.n == 0 { 0. } else { self.n as f32 * TILE_W + (self.n - 1) as f32 * GAP }
    }

    /// Zero before the row has been measured.
    pub fn max_offset(&self) -> f32 {
        if self.view <= 0. { 0. } else { (self.content() - self.view).max(0.) }
    }

    pub fn landing(&self, left: f32, offset: f32) -> usize {
        let centre = left + offset + TILE_W / 2.;
        ((centre / PITCH).floor().max(0.) as usize).min(self.n.saturating_sub(1))
    }

    /// Keeps the tile clear of the fade on every side that has more tiles beyond it.
    pub fn reveal(&self, index: usize, offset: f32) -> f32 {
        let (left, right) = (self.left(index), self.left(index) + TILE_W);
        let before = if index > 0 { FADE_W } else { 0. };
        let after = if index + 1 < self.n { FADE_W } else { 0. };
        let target = if left - before < offset {
            left - before
        } else if right + after > offset + self.view {
            right + after - self.view
        } else {
            offset
        };
        target.clamp(0., self.max_offset())
    }

    pub fn fades(&self, offset: f32) -> (f32, f32) {
        let ramp = |hidden: f32| (hidden / FADE_RAMP).clamp(0., 1.);
        (ramp(offset), ramp(self.max_offset() - offset))
    }
}

pub(super) fn slots_moved(index: usize, from: usize, to: usize) -> f32 {
    if from < to && index > from && index <= to {
        -1.
    } else if to < from && index >= to && index < from {
        1.
    } else {
        0.
    }
}

/// Negative toward the start, in pt/s.
pub(super) fn edge_speed(pointer: f32, view: f32) -> f32 {
    if pointer < EDGE_ZONE {
        -EDGE_SPEED * ((EDGE_ZONE - pointer) / EDGE_ZONE).clamp(0., 1.)
    } else if pointer > view - EDGE_ZONE {
        EDGE_SPEED * ((pointer - (view - EDGE_ZONE)) / EDGE_ZONE).clamp(0., 1.)
    } else {
        0.
    }
}

/// The stronger axis wins, so a vertical wheel scrolls the row sideways and a diagonal swipe does not wobble. Positive
/// moves the content right (toward the start).
pub(super) fn wheel_delta(dx: f32, dy: f32) -> f32 {
    if dx.abs() >= dy.abs() { dx } else { dy }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: f32 = 672.;

    #[test]
    fn fades_follow_what_is_hidden_behind_each_edge() {
        let l = Layout::new(VIEW, 5);
        assert_eq!(l.fades(0.), (0., 1.), "start: only the right edge has more");
        assert_eq!(l.fades(228.), (1., 1.));
        assert_eq!(l.fades(456.), (1., 0.), "end: only the left edge has more");
        assert_eq!(l.fades(6.).0, 0.25, "a fade grows with the distance, so it never pops");
        assert_eq!(Layout::new(VIEW, 3).fades(0.), (0., 0.));
    }

    #[test]
    fn tiles_keep_their_width_and_the_row_scrolls_only_when_they_do_not_fit() {
        assert_eq!(Layout::new(VIEW, 3).max_offset(), 0.);
        // `design:settings/cloud-models/compact-window`: 500 wide, less the plus button and its gap.
        let narrow = Layout::new(452., 3);
        assert_eq!(narrow.max_offset(), 672. - 452.);
        assert_eq!(narrow.fades(0.), (0., 1.), "the third tile shows under the fade at the right edge");
        let wide = Layout::new(1000., 2);
        assert_eq!((wide.content(), wide.max_offset(), wide.fades(0.)), (444., 0., (0., 0.)));
        assert_eq!(Layout::new(0., 5).max_offset(), 0.);
    }

    #[test]
    fn revealing_a_tile_scrolls_the_least_and_keeps_it_clear_of_the_fades() {
        let l = Layout::new(VIEW, 5);
        assert_eq!(l.reveal(0, 228.), 0., "the first tile has nothing before it");
        assert_eq!(l.reveal(4, 0.), 456., "the last tile ends flush with the row");
        assert_eq!(l.reveal(3, 0.), 276., "a tile in the middle keeps 48 pt clear of the fade behind it");
        assert_eq!(l.reveal(1, 228.), 180., "the offset that shows it with 48 pt before it");
        assert_eq!(l.reveal(2, 228.), 228., "already clear: nothing moves");
        assert_eq!(Layout::new(VIEW, 3).reveal(2, 0.), 0., "a row that fits never scrolls");
    }

    #[test]
    fn a_dragged_tile_lands_in_the_slot_under_its_centre() {
        let l = Layout::new(VIEW, 5);
        // The design's drag specimen: the tile is 446 from the left edge of the row, centre 554, slot 2 spans 456..684.
        assert_eq!(l.landing(446., 0.), 2);
        assert_eq!(l.landing(-40., 0.), 0, "past the start");
        assert_eq!(l.landing(1000., 0.), 4, "past the end");
        assert_eq!(l.landing(0., 456.), 2, "the same spot over scrolled content");
    }

    #[test]
    fn the_tiles_between_the_old_and_the_new_slot_make_room() {
        let moved: Vec<f32> = (0..5).map(|i| slots_moved(i, 1, 3)).collect();
        assert_eq!(moved, [0., 0., -1., -1., 0.]);
        let moved: Vec<f32> = (0..5).map(|i| slots_moved(i, 3, 1)).collect();
        assert_eq!(moved, [0., 1., 1., 0., 0.]);
        assert!((0..5).all(|i| slots_moved(i, 2, 2) == 0.), "no move, no room to make");
    }

    #[test]
    fn dragging_near_an_edge_scrolls_that_way_and_faster_the_closer_it_gets() {
        assert_eq!(edge_speed(336., VIEW), 0.);
        assert!(edge_speed(24., VIEW) < 0. && edge_speed(24., VIEW) > edge_speed(0., VIEW));
        assert!(edge_speed(VIEW - 24., VIEW) > 0.);
        assert_eq!(edge_speed(-30., VIEW), -EDGE_SPEED, "beyond the edge is the top speed, not more");
    }

    #[test]
    fn the_stronger_axis_scrolls_the_row() {
        assert_eq!(wheel_delta(0., -30.), -30., "a plain vertical wheel");
        assert_eq!(wheel_delta(12., -3.), 12., "a sideways swipe");
        assert_eq!(wheel_delta(-4., 9.), 9.);
    }
}
