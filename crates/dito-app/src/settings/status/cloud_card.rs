use dito_core::config::{CloudPolishConfiguration, CloudPolishTarget, EngineConfig, Source};
use dito_core::i18n::Tr;
use dito_core::provider::configured_asr::CloudAsrConfigurations;
use dito_core::secrets::Role;
use gpui_kit::*;

use super::engine_card::{EngineParts, detail_grid, hint, model_line};
use super::page::StatusPage;
use crate::settings::{Ctx, role};
use crate::ui::{self, StatusTone, TestState};

/// Local wall-clock "HH:MM" stamp for the last check; offset comes from the platform.
fn hh_mm(epoch: Option<u64>) -> String {
    match epoch {
        None => "—".into(),
        Some(e) => {
            let local = (e as i64 + dito_platform::local_utc_offset_secs() as i64).rem_euclid(86_400);
            format!("{:02}:{:02}", local / 3600, (local % 3600) / 60)
        }
    }
}

fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest).to_string()
}

/// Azure accounts surface as the resource name inside the endpoint host.
fn azure_resource(endpoint: &str) -> String {
    let host = url_host(endpoint);
    host.strip_suffix(".openai.azure.com").unwrap_or(&host).to_string()
}
impl StatusPage {
    pub(super) fn cloud_engine_card(&self, c: &Ctx, role: Role, e: &EngineConfig) -> EngineParts {
        let p = &c.p;
        let idx = role::index(role);
        let asr_configurations = role::cloud_asr_configurations(role, e);
        let asr_configurations = asr_configurations.as_deref();
        let polish_configurations = e.polish_configurations(c.lang);
        let polish_configuration = polish_configurations.active();
        let name = configuration_name(asr_configurations, polish_configuration);
        let (label, tone) = match &self.checks[idx] {
            TestState::Ok(_) => (c.t(Tr::ModelsConnected), StatusTone::Success),
            TestState::Failed(_) => (c.t(Tr::ModelsConnectionError), StatusTone::Danger),
            TestState::Running | TestState::Idle => (c.t(Tr::ModelsChecking), StatusTone::Warning),
        };
        let chip = ui::status_chip(p, label, tone);
        let mut body: Vec<AnyElement> = vec![
            model_line(c, configuration_model(asr_configurations, polish_configuration).to_owned(), Source::Provider)
                .into_any_element(),
        ];
        let mut error = None;
        let mut cells = vec![
            (c.t(Tr::StatusProvider), name.to_string()),
            endpoint_cell(c, asr_configurations, polish_configuration),
        ];
        match &self.checks[idx] {
            TestState::Ok(ms) => {
                cells.push((c.t(Tr::ModelsLatency), format!("{ms} ms")));
                cells.push((c.t(Tr::ModelsLastChecked), hh_mm(self.checked_at[idx])));
            }
            TestState::Failed(msg) => {
                cells.push((c.t(Tr::ModelsLatency), "—".into()));
                let kind_txt = msg.split(':').next().unwrap_or("—").trim().to_string();
                cells.push((c.t(Tr::HudError), kind_txt));
                error = Some(
                    hint(p, msg.clone().into())
                        .text_color(if p.dark { p.danger } else { p.danger_text })
                        .into_any_element(),
                );
            }
            TestState::Running | TestState::Idle => {
                cells.push((c.t(Tr::ModelsLatency), "—".into()));
                cells.push((c.t(Tr::ModelsLastChecked), "—".into()));
            }
        }
        body.push(detail_grid(p, cells).into_any_element());
        body.extend(error);
        (chip, body)
    }
}

/// The cloud speech or cloud polish configuration the engine runs, by the name it was given.
fn configuration_name<'a>(
    asr_configurations: Option<&'a CloudAsrConfigurations>,
    polish_configuration: Option<&'a CloudPolishConfiguration>,
) -> &'a str {
    match asr_configurations {
        Some(asr_configurations) => asr_configurations.active_name().unwrap_or("—"),
        None => polish_configuration.map_or("—", |c| c.name.as_str()),
    }
}

fn configuration_model<'a>(
    asr_configurations: Option<&'a CloudAsrConfigurations>,
    polish_configuration: Option<&'a CloudPolishConfiguration>,
) -> &'a str {
    if let Some(asr_configurations) = asr_configurations {
        return asr_configurations.active()["connection"]["model"].as_str().unwrap_or("");
    }
    polish_configuration.map_or("", |c| c.target.model())
}

/// The card's second fact: where the cloud engine is reached.
fn endpoint_cell(
    c: &Ctx,
    asr_configurations: Option<&CloudAsrConfigurations>,
    polish_configuration: Option<&CloudPolishConfiguration>,
) -> (SharedString, String) {
    if let Some(asr_configurations) = asr_configurations {
        return (
            c.t(Tr::ModelsEndpoint),
            url_host(asr_configurations.active()["connection"]["base_url"].as_str().unwrap_or("")),
        );
    }
    match polish_configuration.map(|c| &c.target) {
        Some(CloudPolishTarget::Azure { endpoint, .. }) => {
            let resource = azure_resource(endpoint);
            (c.t(Tr::ModelsResource), if resource.is_empty() { "—".into() } else { resource })
        }
        Some(CloudPolishTarget::Bedrock { region, .. }) => (c.t(Tr::ModelsRegion), region.clone()),
        Some(CloudPolishTarget::Compat { base_url, .. }) => {
            let host = url_host(base_url);
            (c.t(Tr::ModelsEndpoint), if host.is_empty() { "—".into() } else { host })
        }
        None => (c.t(Tr::ModelsEndpoint), "—".into()),
    }
}
