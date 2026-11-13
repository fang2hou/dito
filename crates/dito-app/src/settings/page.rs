use dito_core::i18n::Tr;
use gpui_kit::*;

use crate::assets::Ic;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Transcription,
    General,
    Models,
    Wordbook,
    Prompts,
    History,
    Stats,
    Status,
}

impl Page {
    pub fn label(self) -> Tr {
        match self {
            Self::Transcription => Tr::NavTranscription,
            Self::General => Tr::NavGeneral,
            Self::Models => Tr::NavModels,
            Self::Wordbook => Tr::NavWordbook,
            Self::Prompts => Tr::NavPrompts,
            Self::History => Tr::NavHistory,
            Self::Stats => Tr::NavStats,
            Self::Status => Tr::NavStatus,
        }
    }

    pub fn english(self) -> &'static str {
        match self {
            Self::Transcription => "Transcription",
            Self::General => "General",
            Self::Models => "Models",
            Self::Wordbook => "Wordbook",
            Self::Prompts => "Prompts",
            Self::History => "History",
            Self::Stats => "Stats",
            Self::Status => "Status",
        }
    }

    /// In sidebar order.
    pub const ALL: [Page; 8] = [
        Self::Transcription,
        Self::General,
        Self::Models,
        Self::Wordbook,
        Self::Prompts,
        Self::History,
        Self::Stats,
        Self::Status,
    ];

    pub(super) fn icon(self) -> Ic {
        match self {
            Self::Transcription => Ic::AudioLines,
            Self::General => Ic::Sliders,
            Self::Models => Ic::Cpu,
            Self::Wordbook => Ic::BookOpen,
            Self::Prompts => Ic::Sparkles,
            Self::History => Ic::Clock,
            Self::Stats => Ic::Chart,
            Self::Status => Ic::Activity,
        }
    }
}

pub(super) const NAV_H: f32 = 44.;
pub(super) const NAV_SECTION_H: f32 = 36.;
pub(super) const NAV_GAP: f32 = 4.;
/// Below this width the sidebar collapses into an icon rail on its own, never as a user toggle: small windows reserve
/// more space for content. Expanding again needs RAIL_EXPAND so dragging across the boundary cannot flicker. Row
/// positions relative to the nav column are identical in both modes, so the nav plate and hover state survive the
/// collapse without replaying.
pub(super) const RAIL_COLLAPSE: f32 = 1024.;
pub(super) const RAIL_EXPAND: f32 = 1088.;
pub(super) const RAIL_MS: u64 = 180;
pub(super) const RAIL_WIDTH: f32 = 76.;
pub(super) const RAIL_PADDING: f32 = (RAIL_WIDTH - 52.) / 2.;

pub(super) fn traffic_light_position(rail: bool) -> Point<Pixels> {
    // The compact rail is slightly narrower than the fully symmetric button-group envelope.
    let inset = px(if rail { 8. } else { 20. });
    point(inset, inset)
}

pub(super) const fn sidebar_top_height(rail: bool) -> f32 {
    if rail { 36. } else { 54. }
}

/// Top of the collapsed nav column: drag strip + brand slot (51) + two gaps (4 each).
pub(super) const NAV_COL_Y: f32 = sidebar_top_height(true) + 51. + 2. * NAV_GAP;

/// The sidebar's sections, each a header and its pages, in order.
pub(super) const SECTIONS: [(Tr, &[Page]); 3] = [
    (Tr::NavFeatures, &[Page::Transcription]),
    (Tr::NavSettings, &[Page::General, Page::Models, Page::Wordbook, Page::Prompts]),
    (Tr::NavActivity, &[Page::History, Page::Stats, Page::Status]),
];

/// Each section header, then its items in order.
pub(super) fn nav_y(page: Page) -> f32 {
    let step = NAV_H + NAV_GAP;
    let header = NAV_SECTION_H + NAV_GAP;
    let mut y = 0.;
    for (_, pages) in SECTIONS {
        y += header;
        for p in pages {
            if *p == page {
                return y;
            }
            y += step;
        }
    }
    unreachable!("every page is in a section")
}

#[cfg(test)]
mod tests {
    use super::{Page, SECTIONS};

    #[test]
    fn every_page_is_in_one_section_in_sidebar_order() {
        let listed: Vec<Page> = SECTIONS.iter().flat_map(|(_, pages)| pages.iter().copied()).collect();
        assert_eq!(listed, Page::ALL);
    }
}
