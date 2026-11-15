use dito_core::i18n::Tr;

use crate::app_shell::NavPage;
use crate::assets::Ic;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    General,
    Models,
    Wordbook,
    Prompts,
    History,
    Stats,
    Status,
}

impl Page {
    pub fn english(self) -> &'static str {
        match self {
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
    pub const ALL: [Page; 7] =
        [Self::General, Self::Models, Self::Wordbook, Self::Prompts, Self::History, Self::Stats, Self::Status];
}

impl NavPage for Page {
    fn label(self) -> Tr {
        match self {
            Self::General => Tr::NavGeneral,
            Self::Models => Tr::NavModels,
            Self::Wordbook => Tr::NavWordbook,
            Self::Prompts => Tr::NavPrompts,
            Self::History => Tr::NavHistory,
            Self::Stats => Tr::NavStats,
            Self::Status => Tr::NavStatus,
        }
    }

    fn icon(self) -> Ic {
        match self {
            Self::General => Ic::Sliders,
            Self::Models => Ic::Cpu,
            Self::Wordbook => Ic::BookOpen,
            Self::Prompts => Ic::Sparkles,
            Self::History => Ic::Clock,
            Self::Stats => Ic::Chart,
            Self::Status => Ic::Activity,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn sections() -> &'static [(Tr, &'static [Page])] {
        &SECTIONS
    }
}

/// The sidebar's sections, each a header and its pages, in order.
static SECTIONS: [(Tr, &[Page]); 2] = [
    (Tr::NavSettings, &[Page::General, Page::Models, Page::Wordbook, Page::Prompts]),
    (Tr::NavActivity, &[Page::History, Page::Stats, Page::Status]),
];

#[cfg(test)]
mod tests {
    use super::{Page, SECTIONS};
    use crate::app_shell::NavPage;

    #[test]
    fn every_page_is_in_one_section_in_sidebar_order() {
        let listed: Vec<Page> = SECTIONS.iter().flat_map(|(_, pages)| pages.iter().copied()).collect();
        assert_eq!(listed, Page::ALL);
        assert_eq!(Page::all(), Page::ALL);
    }
}
