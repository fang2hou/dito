//! The nav model the Settings and Tools windows share: page labels and icons, the section list, and
//! the geometry the sidebar lays its rows out with.

use dito_core::i18n::Tr;
use gpui_kit::*;

use super::Chrome;
use crate::assets::Ic;
use crate::theme::Palette;

pub(crate) trait NavPage: Copy + Eq + 'static {
    fn label(self) -> Tr;
    fn icon(self) -> Ic;
    /// Stable within its window: element ids and rendering order derive from it.
    fn index(self) -> usize;
    /// The sidebar's sections, each a header and its pages, in order.
    fn sections() -> &'static [(Tr, &'static [Self])];

    fn all() -> Vec<Self> {
        Self::sections().iter().flat_map(|(_, pages)| pages.iter().copied()).collect()
    }
}

pub(crate) trait NavHost: Sized + 'static {
    type Page: NavPage;

    fn chrome(&self) -> &Chrome;
    fn nav_page(&self) -> Self::Page;
    fn select_page(&mut self, page: Self::Page, cx: &mut Context<Self>);
    /// Color of the dot at the right end of the page's row (a live session), if there is one.
    fn nav_badge(&self, page: Self::Page, p: &Palette) -> Option<Hsla> {
        let _ = (page, p);
        None
    }
}

pub(crate) const NAV_H: f32 = 44.;
pub(crate) const NAV_SECTION_H: f32 = 36.;
pub(crate) const NAV_GAP: f32 = 4.;
/// Below this width the sidebar collapses into an icon rail on its own, never as a user toggle: small windows reserve
/// more space for content. Expanding again needs RAIL_EXPAND so dragging across the boundary cannot flicker. Row
/// positions relative to the nav column are identical in both modes, so the nav plate and hover state survive the
/// collapse without replaying.
pub(crate) const RAIL_COLLAPSE: f32 = 1024.;
pub(crate) const RAIL_EXPAND: f32 = 1088.;
pub(crate) const RAIL_MS: u64 = 180;
pub(crate) const RAIL_WIDTH: f32 = 76.;
pub(crate) const RAIL_PADDING: f32 = (RAIL_WIDTH - 52.) / 2.;

pub(crate) fn traffic_light_position(rail: bool) -> Point<Pixels> {
    // The compact rail is slightly narrower than the fully symmetric button-group envelope.
    let inset = px(if rail { 8. } else { 20. });
    point(inset, inset)
}

pub(crate) const fn sidebar_top_height(rail: bool) -> f32 {
    if rail { 36. } else { 54. }
}

/// Top of the collapsed nav column: drag strip + brand slot (51) + two gaps (4 each).
pub(crate) const NAV_COL_Y: f32 = sidebar_top_height(true) + 51. + 2. * NAV_GAP;

/// Each section header, then its items in order.
pub(crate) fn nav_y<P: NavPage>(page: P) -> f32 {
    let step = NAV_H + NAV_GAP;
    let header = NAV_SECTION_H + NAV_GAP;
    let mut y = 0.;
    for (_, pages) in P::sections() {
        y += header;
        for p in pages.iter() {
            if *p == page {
                return y;
            }
            y += step;
        }
    }
    unreachable!("every page is in a section")
}
