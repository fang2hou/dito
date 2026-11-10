use dito_core::config::{EngineConfig, PolishConfiguration, PolishTarget, Source};
use dito_core::i18n::Tr;
use dito_core::provider::configured_asr::AsrProfiles;
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
    pub(super) fn provider_engine_card(&self, c: &Ctx, role: Role, e: &EngineConfig) -> EngineParts {
        let p = &c.p;
        let idx = role::index(role);
        let profiles = role::cloud_profiles(role, e);
        let profiles = profiles.as_deref();
        let configurations = e.polish_configurations(c.lang);
        let configuration = configurations.active();
        let name = provider_name(profiles, configuration);
        let (label, tone) = match &self.checks[idx] {
            TestState::Ok(_) => (c.t(Tr::ModelsConnected), StatusTone::Success),
            TestState::Failed(_) => (c.t(Tr::ModelsConnectionError), StatusTone::Danger),
            TestState::Running | TestState::Idle => (c.t(Tr::ModelsChecking), StatusTone::Warning),
        };
        let chip = ui::status_chip(p, label, tone);
        let mut body: Vec<AnyElement> = vec![
            model_line(c, provider_model(profiles, configuration).to_owned(), Source::Provider).into_any_element(),
        ];
        let mut error = None;
        let mut cells = vec![(c.t(Tr::StatusProvider), name.to_string()), endpoint_cell(c, profiles, configuration)];
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

/// The speech profile or polish configuration the engine runs, by the name it was given.
fn provider_name<'a>(profiles: Option<&'a AsrProfiles>, configuration: Option<&'a PolishConfiguration>) -> &'a str {
    match profiles {
        Some(profiles) => profiles.active_name().unwrap_or("—"),
        None => configuration.map_or("—", |c| c.name.as_str()),
    }
}

fn provider_model<'a>(profiles: Option<&'a AsrProfiles>, configuration: Option<&'a PolishConfiguration>) -> &'a str {
    if let Some(profiles) = profiles {
        return profiles.active()["connection"]["model"].as_str().unwrap_or("");
    }
    configuration.map_or("", |c| c.target.model())
}

/// The card's second fact: where the provider is reached.
fn endpoint_cell(
    c: &Ctx,
    profiles: Option<&AsrProfiles>,
    configuration: Option<&PolishConfiguration>,
) -> (SharedString, String) {
    if let Some(profiles) = profiles {
        return (c.t(Tr::ModelsEndpoint), url_host(profiles.active()["connection"]["base_url"].as_str().unwrap_or("")));
    }
    match configuration.map(|c| &c.target) {
        Some(PolishTarget::Azure { endpoint, .. }) => {
            let resource = azure_resource(endpoint);
            (c.t(Tr::ModelsResource), if resource.is_empty() { "—".into() } else { resource })
        }
        Some(PolishTarget::Bedrock { region, .. }) => (c.t(Tr::ModelsRegion), region.clone()),
        Some(PolishTarget::Compat { base_url, .. }) => {
            let host = url_host(base_url);
            (c.t(Tr::ModelsEndpoint), if host.is_empty() { "—".into() } else { host })
        }
        None => (c.t(Tr::ModelsEndpoint), "—".into()),
    }
}
