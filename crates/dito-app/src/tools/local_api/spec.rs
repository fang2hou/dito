//! What the Local API page documents: each endpoint, tool and resource with its request and response, and the Markdown
//! that "copy specification" puts on the clipboard. The text never holds the access token: it shows a placeholder, so a
//! copied spec can be pasted anywhere (ADR-0008).

use dito_core::i18n::Tr;
use dito_core::local_api::{GENERATE_TEXT, LIST_LOCAL_MODELS, MODELS_RESOURCE, tool_schemas};

use crate::ui::MethodKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tab {
    Api,
    Mcp,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Item {
    Models,
    Chat,
    Transcriptions,
    GenerateText,
    ListModels,
    ModelsResource,
}

impl Item {
    pub(super) const API: [Item; 3] = [Self::Models, Self::Chat, Self::Transcriptions];
    pub(super) const MCP: [Item; 3] = [Self::GenerateText, Self::ListModels, Self::ModelsResource];

    pub(super) fn of(tab: Tab) -> &'static [Item] {
        match tab {
            Tab::Api => &Self::API,
            Tab::Mcp => &Self::MCP,
        }
    }

    pub(super) fn kind(self) -> MethodKind {
        match self {
            Self::Models => MethodKind::Get,
            Self::Chat | Self::Transcriptions => MethodKind::Post,
            Self::GenerateText | Self::ListModels | Self::ModelsResource => MethodKind::Neutral,
        }
    }

    pub(super) fn kind_label(self) -> Option<Tr> {
        match self {
            Self::Models | Self::Chat | Self::Transcriptions => None,
            Self::GenerateText | Self::ListModels => Some(Tr::LocalApiKindTool),
            Self::ModelsResource => Some(Tr::LocalApiKindResource),
        }
    }

    pub(super) fn path(self) -> &'static str {
        match self {
            Self::Models => "/v1/models",
            Self::Chat => "/v1/chat/completions",
            Self::Transcriptions => "/v1/audio/transcriptions",
            Self::GenerateText => GENERATE_TEXT,
            Self::ListModels => LIST_LOCAL_MODELS,
            Self::ModelsResource => MODELS_RESOURCE,
        }
    }

    pub(super) fn summary(self) -> Tr {
        match self {
            Self::Models => Tr::LocalApiEndpointModels,
            Self::Chat => Tr::LocalApiEndpointChat,
            Self::Transcriptions => Tr::LocalApiEndpointTranscriptions,
            Self::GenerateText => Tr::LocalApiToolGenerate,
            Self::ListModels => Tr::LocalApiToolListModels,
            Self::ModelsResource => Tr::LocalApiResourceModels,
        }
    }

    fn description(self) -> Tr {
        match self {
            Self::Models => Tr::LocalApiDescribeModels,
            Self::Chat => Tr::LocalApiDescribeChat,
            Self::Transcriptions => Tr::LocalApiDescribeTranscriptions,
            Self::GenerateText => Tr::LocalApiDescribeGenerate,
            Self::ListModels => Tr::LocalApiDescribeListModels,
            Self::ModelsResource => Tr::LocalApiDescribeResource,
        }
    }
}

/// What a reference needs to know about the running service.
pub(super) struct Facts {
    pub(super) base_url: String,
    pub(super) text_model: String,
    pub(super) speech_model: String,
    pub(super) api_any_token: bool,
    pub(super) mcp_any_token: bool,
}

pub(super) struct Block {
    pub(super) label: String,
    pub(super) body: String,
    pub(super) fence: &'static str,
}

pub(super) struct Reference {
    pub(super) item: Item,
    pub(super) kind: String,
    pub(super) description: String,
    pub(super) auth: String,
    pub(super) model: Option<String>,
    pub(super) blocks: Vec<Block>,
    /// A command that calls the endpoint. Only the copied Markdown has it.
    pub(super) curl: Option<String>,
}

fn json(value: &str) -> String {
    serde_json::to_string(value).expect("a string encodes as JSON")
}

pub(super) fn reference(item: Item, facts: &Facts, t: &dyn Fn(Tr) -> String) -> Reference {
    let placeholder = t(Tr::LocalApiRefTokenPlaceholder);
    let any_token = match item {
        Item::Models | Item::Chat | Item::Transcriptions => facts.api_any_token,
        Item::GenerateText | Item::ListModels | Item::ModelsResource => facts.mcp_any_token,
    };
    let auth = if any_token { t(Tr::LocalApiAnyToken) } else { format!("Authorization: Bearer {placeholder}") };
    let bearer = if any_token { String::new() } else { format!(" \\\n  -H \"Authorization: Bearer {placeholder}\"") };
    let (model, kind) = (
        match item {
            Item::Chat | Item::GenerateText => Some(facts.text_model.clone()),
            Item::Transcriptions => Some(facts.speech_model.clone()),
            _ => None,
        },
        item.kind_label().map_or_else(|| if item.kind() == MethodKind::Get { "GET" } else { "POST" }.to_owned(), t),
    );
    let block = |label: Tr, body: String| Block { label: t(label), body, fence: "json" };
    let (blocks, curl) = match item {
        Item::Models => (
            vec![block(
                Tr::LocalApiRefResponse,
                format!(
                    "{{\n  \"object\": \"list\",\n  \"data\": [\n    {{ \"id\": {}, \"object\": \"model\", \"role\": \"text\" }},\n    {{ \"id\": {}, \"object\": \"model\", \"role\": \"speech\" }}\n  ]\n}}",
                    json(&facts.text_model),
                    json(&facts.speech_model)
                ),
            )],
            Some(format!("curl {}/v1/models{bearer}", facts.base_url)),
        ),
        Item::Chat => {
            let request = format!(
                "{{\n  \"model\": {},\n  \"messages\": [{{ \"role\": \"user\", \"content\": {} }}],\n  \"stream\": true\n}}",
                json(&facts.text_model),
                json(&t(Tr::LocalApiSampleMessage))
            );
            let curl = format!(
                "curl {}/v1/chat/completions{bearer} \\\n  -H \"Content-Type: application/json\" \\\n  -d '{}'",
                facts.base_url,
                request.replace('\n', " ").replace("  ", "")
            );
            (
                vec![
                    block(Tr::LocalApiRefRequest, request),
                    block(
                        Tr::LocalApiRefResponseJson,
                        "{\n  \"choices\": [{ \"message\": { \"role\": \"assistant\", \"content\": \"…\" } }]\n}"
                            .into(),
                    ),
                ],
                Some(curl),
            )
        }
        Item::Transcriptions => {
            let request = format!(
                "POST /v1/audio/transcriptions\nContent-Type: multipart/form-data\n\nfile=@speech.wav\nmodel={}\nlanguage=ja\nresponse_format=json",
                facts.speech_model
            );
            (
                vec![
                    Block { label: t(Tr::LocalApiRefRequest), body: request, fence: "http" },
                    block(Tr::LocalApiRefResponse, "{\n  \"text\": \"…\"\n}".into()),
                ],
                Some(format!(
                    "curl {}/v1/audio/transcriptions{bearer} \\\n  -F file=@speech.wav \\\n  -F model={}",
                    facts.base_url, facts.speech_model
                )),
            )
        }
        Item::GenerateText | Item::ListModels => {
            let schemas = tool_schemas();
            let schema = schemas.iter().find(|(name, _)| *name == item.path()).map(|(_, schema)| schema);
            let schema = serde_json::to_string_pretty(&schema).expect("a schema encodes as JSON");
            let arguments = match item {
                Item::GenerateText => format!("{{ \"prompt\": {} }}", json(&t(Tr::LocalApiSamplePrompt))),
                _ => "{}".into(),
            };
            let call = format!(
                "{{\n  \"method\": \"tools/call\",\n  \"params\": {{ \"name\": \"{}\", \"arguments\": {arguments} }}\n}}",
                item.path()
            );
            (vec![block(Tr::LocalApiRefInputSchema, schema), block(Tr::LocalApiRefExample, call)], None)
        }
        Item::ModelsResource => (
            vec![block(
                Tr::LocalApiRefExample,
                format!(
                    "{{\n  \"method\": \"resources/read\",\n  \"params\": {{ \"uri\": \"{MODELS_RESOURCE}\" }}\n}}"
                ),
            )],
            None,
        ),
    };
    Reference { item, kind, description: t(item.description()), auth, model, blocks, curl }
}

pub(super) fn markdown(reference: &Reference, t: &dyn Fn(Tr) -> String) -> String {
    let mut out = format!("## {} {}\n\n{}\n\n", reference.kind, reference.item.path(), reference.description);
    out.push_str(&format!("- {}: `{}`\n", t(Tr::LocalApiRefAuth), reference.auth));
    if let Some(model) = &reference.model {
        out.push_str(&format!("- {}: `{model}`\n", t(Tr::LocalApiRefModel)));
    }
    for block in &reference.blocks {
        out.push_str(&format!("\n### {}\n\n```{}\n{}\n```\n", block.label, block.fence, block.body));
    }
    if let Some(curl) = &reference.curl {
        out.push_str(&format!("\n### {}\n\n```sh\n{curl}\n```\n", t(Tr::LocalApiRefExample)));
    }
    out
}

/// The client configuration the MCP tab shows and the Markdown of the whole tab opens with.
pub(super) fn mcp_client_config(base_url: &str) -> String {
    format!("{{\n  \"mcpServers\": {{ \"dito\": {{ \"url\": \"{base_url}/mcp\" }} }}\n}}")
}

pub(super) fn tab_markdown(tab: Tab, facts: &Facts, t: &dyn Fn(Tr) -> String) -> String {
    let mut out = format!("# {}\n\n", t(Tr::LocalApiDocTitle));
    match tab {
        Tab::Api => out.push_str(&format!("{}: `{}/v1`\n", t(Tr::LocalApiBaseUrl), facts.base_url)),
        Tab::Mcp => out.push_str(&format!(
            "{}: `{}/mcp`\n\n```json\n{}\n```\n",
            t(Tr::LocalApiMcpUrl),
            facts.base_url,
            mcp_client_config(&facts.base_url)
        )),
    }
    for item in Item::of(tab) {
        out.push('\n');
        out.push_str(&markdown(&reference(*item, facts, t), t));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(any_token: bool) -> Facts {
        Facts {
            base_url: "http://127.0.0.1:17841".into(),
            text_model: "qwen-test".into(),
            speech_model: "speech-test".into(),
            api_any_token: any_token,
            mcp_any_token: any_token,
        }
    }

    fn en(tr: Tr) -> String {
        dito_core::i18n::Lang::En.tr(tr).to_owned()
    }

    #[test]
    fn copied_text_shows_the_token_placeholder_and_the_models_in_use() {
        let text = tab_markdown(Tab::Api, &facts(false), &en);
        assert!(text.contains("Authorization: Bearer <access token>"), "{text}");
        assert!(text.contains("qwen-test") && text.contains("speech-test"));
        assert!(text.contains("curl http://127.0.0.1:17841/v1/chat/completions"));
        for item in Item::API {
            assert!(text.contains(item.path()), "{item:?}");
        }
    }

    #[test]
    fn a_service_that_accepts_any_token_documents_calls_without_one() {
        let text = tab_markdown(Tab::Api, &facts(true), &en);
        assert!(!text.contains("Authorization"), "{text}");
        assert!(text.contains("Any token is accepted"));
    }

    #[test]
    fn the_request_samples_are_valid_json_a_client_can_send() {
        for item in Item::API.into_iter().chain(Item::MCP) {
            let reference = reference(item, &facts(false), &en);
            for block in reference.blocks.iter().filter(|b| b.fence == "json") {
                assert!(serde_json::from_str::<serde_json::Value>(&block.body).is_ok(), "{item:?}: {}", block.body);
            }
        }
        let chat = reference(Item::Chat, &facts(false), &en);
        let request: serde_json::Value = serde_json::from_str(&chat.blocks[0].body).unwrap();
        assert_eq!(request["model"], "qwen-test");
        assert_eq!(request["stream"], true);
    }

    #[test]
    fn the_mcp_tab_opens_with_the_client_configuration() {
        let text = tab_markdown(Tab::Mcp, &facts(true), &en);
        assert!(text.contains("\"url\": \"http://127.0.0.1:17841/mcp\""), "{text}");
        for item in Item::MCP {
            assert!(text.contains(item.path()), "{item:?}");
        }
    }
}
