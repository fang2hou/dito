//! The numbers of a configuration row (`20.05.13`).

pub(super) const TILE_H: f32 = 64.;
pub(super) const TILE_W: f32 = 200.;
pub(super) const GAP: f32 = 12.;
/// Up to this many tiles share the row equally; from the next one every tile keeps [`TILE_W`].
const EQUAL_MAX: usize = 3;

/// Whether `n` tiles divide the row between them instead of keeping their own width.
pub(super) fn shares_width(n: usize) -> bool {
    n <= EQUAL_MAX
}

/// Width of `n` tiles side by side at their own width.
pub(super) fn content(n: usize) -> f32 {
    n as f32 * TILE_W + n.saturating_sub(1) as f32 * GAP
}
