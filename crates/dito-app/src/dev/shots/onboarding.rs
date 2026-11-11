use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{Appearance, RecordMode};
use gpui_kit::*;

use super::capture::{close_window, render, save, theme_tag};
use crate::hotkeys::CaptureState;
use crate::onboarding::demo::{self, Frozen};
use crate::onboarding::{self, Onboarding};
use crate::services;
use crate::state::AppState;
use crate::ui::Root;

const CUSTOM_SHORTCUT: &str = "Control+Option+Space";
const STEP_COUNT: usize = 7;

pub async fn onboarding_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    demo::freeze(Some(Frozen::Still));
    for appearance in [Appearance::Light, Appearance::Dark] {
        services::save_config(st, |c| c.general.appearance = appearance);
        let Some((window, view)) = cx.update(|cx| onboarding::open_with_view(st.clone(), cx)) else { continue };
        let shots = WizardShots { st, dir, window, view, tag: theme_tag(appearance) };
        shots.capture_all(cx).await;
        close_window(cx, window);
    }
    demo::freeze(None);
}

struct WizardShots<'a> {
    st: &'a Arc<AppState>,
    dir: &'a Path,
    window: WindowHandle<Root>,
    view: Entity<Onboarding>,
    tag: &'static str,
}

impl WizardShots<'_> {
    async fn capture_all(&self, cx: &mut AsyncApp) {
        let tag = self.tag;
        for n in 0..STEP_COUNT {
            self.view.update(cx, |o, cx| o.show_step(n, cx));
            self.shoot(cx, &format!("onboarding-{n}-{tag}.png")).await;
        }
        self.view.update(cx, |o, cx| o.show_provider_form(cx));
        self.shoot(cx, &format!("onboarding-provider-{tag}.png")).await;
        self.shortcut_step_shots(cx).await;
        self.view.update(cx, |o, cx| {
            o.set_mode(RecordMode::Hold, cx);
            o.show_step(2, cx);
        });
        self.shoot(cx, &format!("onboarding-2-hold-{tag}.png")).await;
        self.view.update(cx, |o, cx| o.show_step(6, cx));
        self.shoot(cx, &format!("onboarding-6-hold-{tag}.png")).await;
    }

    async fn shortcut_step_shots(&self, cx: &mut AsyncApp) {
        let (st, tag) = (self.st, self.tag);
        self.view.update(cx, |o, cx| o.show_step(3, cx));
        let before = st.cfg.get().general.shortcut.clone();
        st.hotkeys.capture.send_replace(CaptureState::Waiting);
        self.shoot(cx, &format!("onboarding-3-capturing-{tag}.png")).await;
        services::save_config(st, |c| c.general.shortcut = CUSTOM_SHORTCUT.into());
        st.hotkeys.capture.send_replace(CaptureState::Idle);
        self.shoot(cx, &format!("onboarding-3-custom-{tag}.png")).await;
        services::save_config(st, |c| c.general.shortcut = before);
    }

    async fn shoot(&self, cx: &mut AsyncApp, name: &str) {
        cx.background_executor().timer(Duration::from_millis(500)).await;
        if let Some(img) = render(cx, self.window) {
            save(self.dir, name, img);
        }
    }
}
