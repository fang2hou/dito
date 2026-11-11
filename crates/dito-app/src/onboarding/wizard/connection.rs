use dito_core::config::{CloudAsrConfiguration, CloudAsrConfigurations};
use dito_core::secrets::{Role, asr_account};
use gpui_kit::*;

use crate::services;
use crate::ui::TestState;

use super::Onboarding;

/// The configuration the step starts on: the one in use, or the first when the saved selection names none, which
/// `apply_cloud_asr` then selects.
pub(in crate::onboarding) fn starting_cloud_asr(
    configurations: &CloudAsrConfigurations,
) -> Option<&CloudAsrConfiguration> {
    configurations.active().or_else(|| configurations.items.first())
}

pub(in crate::onboarding) fn connection_fields(configuration: Option<&CloudAsrConfiguration>) -> (String, String) {
    let field = |name: &str| configuration.and_then(|c| c.config["connection"][name].as_str()).unwrap_or("").to_owned();
    (field("base_url"), field("model"))
}

/// Make `id` the configuration in use and set its endpoint and model. `false`, with `configurations` untouched, when
/// no configuration has that id: the one in use would otherwise take the edits.
fn edit_cloud_asr(configurations: &mut CloudAsrConfigurations, id: &str, base_url: String, model: String) -> bool {
    if !configurations.select(id) {
        return false;
    }
    if let Some(config) = configurations.active_mut() {
        config["connection"]["base_url"] = base_url.into();
        config["connection"]["model"] = model.into();
    }
    true
}

impl Onboarding {
    /// The tested configuration changed (endpoint, key or configuration edit): the recorded result no longer
    /// describes it, and any in-flight request must be superseded so its success cannot later overwrite the fresh idle
    /// state for the obsolete configuration.
    pub(in crate::onboarding) fn invalidate_test(&mut self) {
        self.reqs.bump();
        self.test = TestState::Idle;
    }

    /// A step change makes any in-flight connection test stale: entering, leaving or restarting a step supersedes the
    /// pending request, and a "connecting" state whose result is dropped would never clear. A result that already
    /// landed before the change stays.
    pub(in crate::onboarding) fn supersede_pending_test(&mut self) {
        self.reqs.bump();
        if matches!(self.test, TestState::Running) {
            self.test = TestState::Idle;
        }
    }

    pub(in crate::onboarding) fn select_cloud_asr(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_cloud_asr(cx);
        self.cloud_asr_id = id.to_owned();
        let mut p = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        p.select(id);
        self.show_cloud_asr(p.active(), window, cx);
        self.invalidate_test();
        cx.notify();
    }

    fn show_cloud_asr(
        &self,
        configuration: Option<&CloudAsrConfiguration>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (base_url, model) = connection_fields(configuration);
        let key = configuration.and_then(|c| self.st.secrets.get(&c.account())).unwrap_or_default();
        self.endpoint.update(cx, |s, cx| s.set_value(base_url, window, cx));
        self.deployment.update(cx, |s, cx| s.set_value(model, window, cx));
        self.key.update(cx, |s, cx| s.set_value(key, window, cx));
    }

    pub(in crate::onboarding) fn open_connection_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_cloud_asr(cx);
        let editor = cx.new(|cx| crate::settings::asr_editor::AsrEditor::new_dialog(self.st.clone(), window, cx));
        cx.subscribe_in(&editor, window, |this, _, event: &crate::settings::asr_editor::Closed, window, cx| {
            if event.0 {
                let p = this.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
                let starting = starting_cloud_asr(&p);
                this.cloud_asr_id = starting.map(|c| c.id.clone()).unwrap_or_default();
                this.show_cloud_asr(starting, window, cx);
                this.invalidate_test();
            }
            this.editor = None;
            cx.notify();
        })
        .detach();
        self.editor = Some(editor);
        cx.notify();
    }

    /// Nothing is written, key included, when the configuration no longer exists: another one must not take the edits.
    pub(in crate::onboarding) fn apply_cloud_asr(&self, cx: &App) {
        let (id, base, model) = (
            self.cloud_asr_id.clone(),
            self.endpoint.read(cx).value().to_string(),
            self.deployment.read(cx).value().to_string(),
        );
        let mut exists = false;
        services::save_config(&self.st, |c| {
            let p = c.asr.cloud_asr.get_or_insert_with(CloudAsrConfigurations::default);
            exists = edit_cloud_asr(p, &id, base, model);
        });
        if !exists {
            return;
        }
        let key = self.key.read(cx).value().to_string();
        services::store_secret(&self.st, &asr_account(&id), &key);
    }

    pub(in crate::onboarding) fn run_test(&mut self, cx: &mut Context<Self>) {
        let req = self.reqs.begin();
        self.apply_cloud_asr(cx);
        let configurations = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        let edited = configurations.active().filter(|c| c.id == self.cloud_asr_id);
        if edited.is_none_or(|c| dito_core::provider::configured_asr::validate(&c.config).is_err()) {
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

#[cfg(test)]
mod tests {
    // Named imports: a glob of this module would bring in `gpui_kit`'s own `test` attribute.
    use super::{CloudAsrConfigurations, connection_fields, edit_cloud_asr, starting_cloud_asr};

    #[test]
    fn edits_go_to_the_named_configuration_and_select_it() {
        let mut configurations = CloudAsrConfigurations::default();
        assert!(edit_cloud_asr(&mut configurations, "openrouter", "https://x.example".into(), "m".into()));
        let active = configurations.active().unwrap();
        assert_eq!(active.id, "openrouter");
        assert_eq!(connection_fields(Some(active)), ("https://x.example".to_owned(), "m".to_owned()));
        let first = &configurations.items[0];
        assert_ne!(first.config["connection"]["model"], "m", "no other configuration changed");
    }

    #[test]
    fn a_configuration_that_is_gone_takes_no_edits_and_nobody_else_does() {
        let mut configurations = CloudAsrConfigurations::default();
        let before = configurations.clone();
        assert!(!edit_cloud_asr(&mut configurations, "deleted-1", "https://x.example".into(), "m".into()));
        assert_eq!(configurations, before, "neither the selection nor any configuration changed");
    }

    #[test]
    fn a_selection_that_names_nothing_starts_the_step_on_the_first_configuration() {
        let stale = CloudAsrConfigurations { selected: "gone".into(), ..Default::default() };
        assert_eq!(starting_cloud_asr(&stale).map(|c| c.id.as_str()), Some("funasr"));
        let selected = CloudAsrConfigurations { selected: "openrouter".into(), ..Default::default() };
        assert_eq!(starting_cloud_asr(&selected).map(|c| c.id.as_str()), Some("openrouter"));
    }
}
