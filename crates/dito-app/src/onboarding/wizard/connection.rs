use dito_core::secrets::Role;
use gpui_kit::*;

use crate::services;
use crate::ui::TestState;

use super::Onboarding;

impl Onboarding {
    /// The tested configuration changed (endpoint, key or configuration edit): the recorded result no longer
    /// describes it, and any in-flight request must be superseded so its success cannot later overwrite
    /// the fresh idle state for the obsolete configuration.
    pub(in crate::onboarding) fn invalidate_test(&mut self) {
        self.reqs.bump();
        self.test = TestState::Idle;
    }

    /// A step change makes any in-flight connection test stale: entering, leaving or restarting a step
    /// supersedes the pending request, and a "connecting" state whose result is dropped would never clear.
    /// A result that already landed before the change stays.
    pub(in crate::onboarding) fn supersede_pending_test(&mut self) {
        self.reqs.bump();
        if matches!(self.test, TestState::Running) {
            self.test = TestState::Idle;
        }
    }

    /// Switch the cloud speech configuration the step edits to `id`: keep the edits of the old one, then show the new
    /// configuration's endpoint, model and key.
    pub(in crate::onboarding) fn select_cloud_asr(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_cloud_asr(cx);
        self.cloud_asr_id = id.to_owned();
        let mut p = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        p.select(id);
        self.endpoint
            .update(cx, |s, cx| s.set_value(p.active()["connection"]["base_url"].as_str().unwrap_or(""), window, cx));
        self.deployment
            .update(cx, |s, cx| s.set_value(p.active()["connection"]["model"].as_str().unwrap_or(""), window, cx));
        let key = self.st.secrets.get(&p.account()).unwrap_or_default();
        self.key.update(cx, |s, cx| s.set_value(key, window, cx));
        self.invalidate_test();
        cx.notify();
    }

    pub(in crate::onboarding) fn open_connection_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_cloud_asr(cx);
        let editor = cx.new(|cx| crate::settings::asr_editor::AsrEditor::new_dialog(self.st.clone(), window, cx));
        cx.subscribe_in(&editor, window, |this, _, event: &crate::settings::asr_editor::Closed, window, cx| {
            if event.0 {
                let p = this.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
                this.cloud_asr_id = p.selected_id().to_owned();
                this.endpoint.update(cx, |s, cx| {
                    s.set_value(p.active()["connection"]["base_url"].as_str().unwrap_or(""), window, cx)
                });
                this.deployment.update(cx, |s, cx| {
                    s.set_value(p.active()["connection"]["model"].as_str().unwrap_or(""), window, cx)
                });
                this.key
                    .update(cx, |s, cx| s.set_value(this.st.secrets.get(&p.account()).unwrap_or_default(), window, cx));
                this.invalidate_test();
            }
            this.editor = None;
            cx.notify();
        })
        .detach();
        self.editor = Some(editor);
        cx.notify();
    }

    /// Save what the step shows into the configuration it edits, and make that configuration the one in use.
    pub(in crate::onboarding) fn apply_cloud_asr(&self, cx: &App) {
        let (id, base, model) = (
            self.cloud_asr_id.clone(),
            self.endpoint.read(cx).value().to_string(),
            self.deployment.read(cx).value().to_string(),
        );
        services::save_config(&self.st, |c| {
            let p = c.asr.cloud_asr.get_or_insert_with(dito_core::provider::configured_asr::AsrProfiles::default);
            p.select(&id);
            if let Some(v) = p.active_mut() {
                v["connection"]["base_url"] = base.into();
                v["connection"]["model"] = model.into();
            }
        });
        let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        let key = self.key.read(cx).value().to_string();
        services::store_secret(&self.st, &profiles.account(), &key);
    }

    pub(in crate::onboarding) fn run_test(&mut self, cx: &mut Context<Self>) {
        let req = self.reqs.begin();
        self.apply_cloud_asr(cx);
        let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        if dito_core::provider::configured_asr::validate(profiles.active()).is_err() {
            return;
        }
        self.test = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let r = services::test_connection(st, Role::Asr).await;
            let _ = this.update(cx, |t, cx| {
                // A newer request or a step change has superseded this one; its result must not overwrite
                // the current step's request or state.
                if !t.reqs.is_current(req) {
                    return;
                }
                t.test = r.into();
                cx.notify();
            });
        })
        .detach();
    }
}
