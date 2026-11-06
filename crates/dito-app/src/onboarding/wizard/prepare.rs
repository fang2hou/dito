use dito_local::ModelState;
use gpui_kit::*;

use crate::onboarding::steps;
use crate::services;

use super::Onboarding;

/// Where the local-model preparation stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::onboarding) enum PreparePhase {
    /// Nothing is running; 準備を始める is offered.
    Idle,
    Downloading,
    /// A model failed to download; the button offers 再試行.
    Failed,
    Ready,
}

impl Onboarding {
    pub(in crate::onboarding) fn local_ids(&self) -> [String; 2] {
        let cfg = self.st.cfg.get();
        [cfg.asr.local_model.clone(), cfg.polish.engine.local_model.clone()]
    }

    pub(in crate::onboarding) fn prepare_phase(&self) -> PreparePhase {
        let states = self.local_ids().map(|id| self.st.models.state(&id));
        if states.iter().all(|s| *s == ModelState::Installed) {
            PreparePhase::Ready
        } else if states.iter().any(|s| s.is_active())
            || (self.prepare_started && !states.iter().any(|s| matches!(s, ModelState::Failed { .. })))
        {
            PreparePhase::Downloading
        } else if states.iter().any(|s| matches!(s, ModelState::Failed { .. })) {
            PreparePhase::Failed
        } else {
            PreparePhase::Idle
        }
    }

    /// 準備を始める: the only place a local model download begins.
    pub(in crate::onboarding) fn start_prepare(&mut self, cx: &mut Context<Self>) {
        if matches!(self.prepare_phase(), PreparePhase::Downloading | PreparePhase::Ready) {
            return;
        }
        self.prepare_started = true;
        for id in self.local_ids() {
            let _ = self.st.models.start_download(&id);
        }
        cx.notify();
    }

    pub(in crate::onboarding) fn models_bytes(&self, ids: &[&str]) -> u64 {
        ids.iter().filter_map(|id| self.st.models.spec(id)).flat_map(|s| s.files.iter().map(|f| f.size)).sum()
    }

    /// Choose the local ASR + polish pair. It only changes the choice: the download waits for 準備を始める, and the
    /// choice is locked while one runs.
    pub(in crate::onboarding) fn pick_tier(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.prepare_phase() == PreparePhase::Downloading {
            return;
        }
        let t = &steps::TIERS[i];
        services::save_config(&self.st, |c| {
            c.asr.local_model = t.asr.to_string();
            c.polish.engine.local_model = t.polish.to_string();
        });
        cx.notify();
    }
}
