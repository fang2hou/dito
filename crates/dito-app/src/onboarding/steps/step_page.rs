use gpui_kit::*;

use crate::settings::Ctx;

use super::{engine, permissions, prepare, shortcut, try_it, usage};
use crate::onboarding::{Onboarding, Step};

/// What a step shows: its title, description and body column.
pub(in crate::onboarding) struct StepPage {
    pub(in crate::onboarding) title: SharedString,
    pub(in crate::onboarding) desc: SharedString,
    pub(in crate::onboarding) body: Div,
}

impl StepPage {
    /// The page of the wizard's current step; the welcome page is drawn without one.
    pub(in crate::onboarding) fn current(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> Self {
        match w.step {
            Step::Welcome => Self { title: SharedString::default(), desc: SharedString::default(), body: div() },
            Step::Permissions => permissions::page(w, c, cx),
            Step::Usage => usage::page(w, c, cx),
            Step::Shortcut => shortcut::page(w, c, cx),
            Step::Engine => engine::page(w, c, cx),
            Step::Prepare if w.local => prepare::local(w, c, cx),
            Step::Prepare => prepare::provider(w, c, cx),
            Step::Try => try_it::page(w, c, cx),
        }
    }
}
