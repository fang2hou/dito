use std::time::Duration;

use dito_core::config::RecordMode;
use dito_core::controller::HudState;
use dito_platform::{Status, permissions};
use gpui_kit::*;

use crate::hotkeys::ShortcutTarget;
use crate::state::AppState;
use crate::ui;

use super::{Onboarding, PERMS, Step};

impl Onboarding {
    pub(super) fn watch_capture(st: &AppState, cx: &mut Context<Self>) {
        ui::observe(st.hotkeys.capture.subscribe(), cx, |t, capture, cx| {
            t.capturing = capture.is_waiting_for(ShortcutTarget::Dictation);
            cx.notify();
        });
    }

    pub(super) fn watch_models(st: &AppState, cx: &mut Context<Self>) {
        ui::notify_on_change(st.models.subscribe(), cx);
    }

    pub(super) fn watch_try_progress(st: &AppState, cx: &mut Context<Self>) {
        ui::observe(st.dictation.hud(), cx, Self::record_try_progress);
    }

    fn record_try_progress(&mut self, state: HudState, cx: &mut Context<Self>) {
        if self.step != Step::Try {
            return;
        }
        // Hold: releasing the key (transcription starts) completes both "speak" and "release".
        let hold = self.mode == RecordMode::Hold;
        match state {
            HudState::Listening { .. } => self.seen[0] = true,
            HudState::Transcribing => {
                self.seen[1] = true;
                self.seen[2] |= hold;
            }
            HudState::Inserted if self.tried_ok => {
                self.seen[2] = true;
            }
            _ => {}
        }
        cx.notify();
    }

    /// Reads the statuses once, so the page never opens on an old answer, then keeps them current for as long as the
    /// caller holds the task.
    pub(super) fn poll_permissions(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.perms = PERMS.map(permissions::status);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1200)).await;
                let perms = PERMS.map(permissions::status);
                if this.update(cx, |t, cx| t.apply_polled(perms, cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn apply_polled(&mut self, perms: [Status; 3], cx: &mut Context<Self>) {
        if self.perms != perms {
            self.perms = perms;
            cx.notify();
        }
    }
}
