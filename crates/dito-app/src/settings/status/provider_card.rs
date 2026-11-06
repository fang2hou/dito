use dito_core::config::{EngineConfig, ProviderKind};
use dito_core::i18n::Tr;
use dito_core::provider::configured_asr::AsrProfiles;
use dito_core::secrets::Role;
use gpui_kit::*;

use super::engine_card::{EngineParts, info_row};
use super::page::StatusPage;
use crate::settings::{Ctx, role};
use crate::ui::{self, TestState, text};

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
        let name = provider_name(c, e);
        let head_extra = Some(text(name, 12., FontWeight::NORMAL, p.muted));
        let (label, tone) = match &self.checks[idx] {
            TestState::Ok(_) => (c.t(Tr::ModelsConnected), p.success),
            TestState::Failed(_) => (c.t(Tr::ModelsConnectionError), p.danger),
            TestState::Running | TestState::Idle => (c.t(Tr::ModelsChecking), p.warning_text),
        };
        let chip = ui::status_chip(label, tone);
        let mut body: Vec<AnyElement> =
            vec![text(provider_model(e).to_owned(), 15., FontWeight::MEDIUM, p.text).into_any_element()];
        let mut cells = vec![(c.t(Tr::ModelsProvider), name.to_string()), endpoint_cell(c, e)];
        match &self.checks[idx] {
            TestState::Ok(ms) => {
                cells.push((c.t(Tr::ModelsLatency), format!("{ms} ms")));
                cells.push((c.t(Tr::ModelsLastChecked), hh_mm(self.checked_at[idx])));
            }
            TestState::Failed(msg) => {
                cells.push((c.t(Tr::ModelsLatency), "—".into()));
                let kind_txt = msg.split(':').next().unwrap_or("—").trim().to_string();
                cells.push((c.t(Tr::HudError), kind_txt));
                body.push(
                    text(msg.clone(), 13., FontWeight::NORMAL, if p.dark { p.danger } else { p.danger_text })
                        .into_any_element(),
                );
            }
            TestState::Running | TestState::Idle => {
                cells.push((c.t(Tr::ModelsLatency), "—".into()));
                cells.push((c.t(Tr::ModelsLastChecked), "—".into()));
            }
        }
        body.push(info_row(p, cells).into_any_element());
        (chip, head_extra, body)
    }
}

fn provider_name(c: &Ctx, e: &EngineConfig) -> &'static str {
    e.cloud_asr
        .as_ref()
        .and_then(|profiles| AsrProfiles::NAMES.get(profiles.selected).copied())
        .unwrap_or_else(|| c.lang.tr(e.provider.kind.label()))
}

fn provider_model(e: &EngineConfig) -> &str {
    if let Some(profiles) = &e.cloud_asr {
        return profiles.active()["connection"]["model"].as_str().unwrap_or("");
    }
    match e.provider.kind {
        ProviderKind::Azure => &e.provider.azure.deployment,
        ProviderKind::Bedrock => &e.provider.bedrock.model_id,
        ProviderKind::Compat => &e.provider.compat.model,
    }
}

/// The card's second fact: where the provider is reached.
fn endpoint_cell(c: &Ctx, e: &EngineConfig) -> (SharedString, String) {
    if let Some(profiles) = &e.cloud_asr {
        return (c.t(Tr::ModelsEndpoint), url_host(profiles.active()["connection"]["base_url"].as_str().unwrap_or("")));
    }
    match e.provider.kind {
        ProviderKind::Azure => {
            let resource = azure_resource(&e.provider.azure.endpoint);
            (c.t(Tr::ModelsResource), if resource.is_empty() { "—".into() } else { resource })
        }
        ProviderKind::Bedrock => (c.t(Tr::ModelsRegion), e.provider.bedrock.region.clone()),
        ProviderKind::Compat => {
            let host = url_host(&e.provider.compat.base_url);
            (c.t(Tr::ModelsEndpoint), if host.is_empty() { "—".into() } else { host })
        }
    }
}
