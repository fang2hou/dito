//! Where the tiles of a configuration row sit and how far the row scrolls. Everything here is plain arithmetic on
//! points; the numbers are the design's (`20.05.13`).

pub(super) const TILE_H: f32 = 64.;
pub(super) const TILE_W: f32 = 200.;
pub(super) const GAP: f32 = 12.;
pub(super) const FADE_W: f32 = 48.;
/// Up to this many tiles share the row equally; from the next one every tile keeps [`TILE_W`] and the row scrolls.
const EQUAL_MAX: usize = 3;
/// A fade shows as much as there is hidden behind its edge: it grows over this distance, so reaching an end
/// never makes it pop out.
const FADE_RAMP: f32 = 24.;

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
    fn the_stronger_axis_scrolls_the_row() {
        assert_eq!(wheel_delta(0., -30.), -30., "a plain vertical wheel");
        assert_eq!(wheel_delta(12., -3.), 12., "a sideways swipe");
        assert_eq!(wheel_delta(-4., 9.), 9.);
    }
}
