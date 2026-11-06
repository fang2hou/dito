//! What the screenshot harness steers on the wizard.

use dito_core::config::RecordMode;
use gpui_kit::*;

use super::{Onboarding, Step};

impl Onboarding {
    pub fn show_step(&mut self, n: usize, cx: &mut Context<Self>) {
        const ALL: [Step; 7] =
            [Step::Welcome, Step::Permissions, Step::Usage, Step::Shortcut, Step::Engine, Step::Prepare, Step::Try];
        self.step = ALL[n];
        self.local = n != 6 || self.local;
        self.supersede_pending_test();
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: RecordMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    pub fn show_provider_form(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Prepare;
        self.local = false;
        self.supersede_pending_test();
        cx.notify();
    }
}
