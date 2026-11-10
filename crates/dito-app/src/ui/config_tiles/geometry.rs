//! Where the tiles of a configuration row sit, how far the row scrolls and which slot a dragged tile lands in.
//! Everything here is plain arithmetic on points; the numbers are the design's (`20.05.13`).

pub(super) const TILE_H: f32 = 64.;
pub(super) const TILE_W: f32 = 200.;
pub(super) const GAP: f32 = 12.;
pub(super) const FADE_W: f32 = 48.;
/// How far the dragged tile floats above the row.
pub(super) const LIFT: f32 = 8.;
/// Up to this many tiles share the row equally; from the next one every tile keeps [`TILE_W`] and the row scrolls.
const EQUAL_MAX: usize = 3;
/// A fade shows as much as there is hidden behind its edge: it grows over this distance, so reaching an end
/// never makes it pop out.
const FADE_RAMP: f32 = 24.;
/// Distance from an edge of the row inside which a dragged tile scrolls it, and the speed right at the edge (pt/s).
const EDGE_ZONE: f32 = 48.;
const EDGE_SPEED: f32 = 600.;

/// The solved geometry of a row of `n` tiles in a scrolling area `view` wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub view: f32,
    pub n: usize,
    /// Width of every tile.
    pub w: f32,
    /// Distance between the left edges of neighbouring tiles.
    pub pitch: f32,
}

impl Layout {
    pub fn new(view: f32, n: usize) -> Self {
        let w = match n {
            1..=EQUAL_MAX => ((view - GAP * (n - 1) as f32) / n as f32).max(0.),
            _ => TILE_W,
        };
        Self { view, n, w, pitch: w + GAP }
    }

    /// Whether the tiles divide the row between them (up to three) instead of keeping their own width.
    pub fn shares_width(&self) -> bool {
        self.n <= EQUAL_MAX
    }

    /// Left edge of tile `index` in the scrolled content.
    pub fn left(&self, index: usize) -> f32 {
        index as f32 * self.pitch
    }

    pub fn content(&self) -> f32 {
        if self.n == 0 { 0. } else { self.n as f32 * self.w + (self.n - 1) as f32 * GAP }
    }

    /// The farthest the content scrolls; zero while everything fits, and before the row has been measured.
    pub fn max_offset(&self) -> f32 {
        if self.view <= 0. { 0. } else { (self.content() - self.view).max(0.) }
    }

    /// The slot a tile whose left edge is `left` from the left edge of the scrolling area lands in.
    pub fn landing(&self, left: f32, offset: f32) -> usize {
        let centre = left + offset + self.w / 2.;
        ((centre / self.pitch).floor().max(0.) as usize).min(self.n.saturating_sub(1))
    }

    /// The offset that shows tile `index` completely: the nearest scroll that keeps it clear of the fade on every
    /// side that has more tiles beyond it. An offset that already does is returned unchanged.
    pub fn reveal(&self, index: usize, offset: f32) -> f32 {
        let (left, right) = (self.left(index), self.left(index) + self.w);
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

    /// Strength of the fade over the left and right edge at `offset`, each 0 to 1.
    pub fn fades(&self, offset: f32) -> (f32, f32) {
        let ramp = |hidden: f32| (hidden / FADE_RAMP).clamp(0., 1.);
        (ramp(offset), ramp(self.max_offset() - offset))
    }
}

/// How far, in slots, tile `index` slides while the tile at `from` is dragged over the slot `to`: the tiles between
/// the two close the gap the dragged tile leaves and open the one it is about to fill.
pub(super) fn slots_moved(index: usize, from: usize, to: usize) -> f32 {
    if from < to && index > from && index <= to {
        -1.
    } else if to < from && index >= to && index < from {
        1.
    } else {
        0.
    }
}

/// Scroll speed (pt/s, negative toward the start) for a dragged tile held `pointer` from the left edge of a scrolling
/// area `view` wide: zero in the middle, up to [`EDGE_SPEED`] at either edge.
pub(super) fn edge_speed(pointer: f32, view: f32) -> f32 {
    if pointer < EDGE_ZONE {
        -EDGE_SPEED * ((EDGE_ZONE - pointer) / EDGE_ZONE).clamp(0., 1.)
    } else if pointer > view - EDGE_ZONE {
        EDGE_SPEED * ((pointer - (view - EDGE_ZONE)) / EDGE_ZONE).clamp(0., 1.)
    } else {
        0.
    }
}

/// How far the content moves for one scroll event: the stronger axis wins, so a vertical wheel scrolls the row
/// sideways and a diagonal swipe does not wobble. Positive moves the content right (toward the start).
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
        assert_eq!(l.fades(188.), (1., 1.));
        assert_eq!(l.fades(376.), (1., 0.), "end: only the left edge has more");
        assert_eq!(l.fades(6.).0, 0.25, "a fade grows with the distance, so it never pops");
        assert_eq!(Layout::new(VIEW, 3).fades(0.), (0., 0.));
    }

    #[test]
    fn revealing_a_tile_scrolls_the_least_and_keeps_it_clear_of_the_fades() {
        let l = Layout::new(VIEW, 5);
        assert_eq!(l.reveal(0, 188.), 0., "the first tile has nothing before it");
        assert_eq!(l.reveal(4, 0.), 376., "the last tile ends flush with the row");
        assert_eq!(l.reveal(3, 0.), 212., "a tile in the middle keeps 48 pt clear of the fade behind it");
        assert_eq!(l.reveal(1, 188.), 164., "the offset that shows it with 48 pt before it");
        assert_eq!(l.reveal(2, 188.), 188., "already clear: nothing moves");
        assert_eq!(Layout::new(VIEW, 3).reveal(2, 0.), 0., "a row that fits never scrolls");
    }

    #[test]
    fn a_dragged_tile_lands_in_the_slot_under_its_centre() {
        let l = Layout::new(VIEW, 5);
        // The design's drag specimen: the tile is 446 from the left edge of the row, centre 546, slot 2 spans 424..636.
        assert_eq!(l.landing(446., 0.), 2);
        assert_eq!(l.landing(-40., 0.), 0, "past the start");
        assert_eq!(l.landing(1000., 0.), 4, "past the end");
        assert_eq!(l.landing(0., 376.), 2, "the same spot over scrolled content");
    }

    #[test]
    fn the_tiles_between_the_old_and_the_new_slot_make_room() {
        // Drag tile 1 over slot 3: tiles 2 and 3 slide left.
        let moved: Vec<f32> = (0..5).map(|i| slots_moved(i, 1, 3)).collect();
        assert_eq!(moved, [0., 0., -1., -1., 0.]);
        // Drag tile 3 over slot 1: tiles 1 and 2 slide right.
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
