//! The MCP endpoint: two tools and one resource over Streamable HTTP (`design:tools/mcp`).

use std::sync::Arc;

use rmcp::ErrorData;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListResourcesResult,
    ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
    Resource, ResourceContents, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use super::models::{model_listing, thinking_for};
use super::server::{Shared, generate};
use crate::provider::{ChatCancel, ChatMessage, ChatRequest, ChatRole};

pub const GENERATE_TEXT: &str = "generate_text";
pub const LIST_LOCAL_MODELS: &str = "list_local_models";
pub const MODELS_RESOURCE: &str = "dito://models";

#[derive(Clone)]
pub(super) struct DitoMcp {
    shared: Arc<Shared>,
}

pub(super) fn service(
    shared: Arc<Shared>,
    cancel: CancellationToken,
) -> StreamableHttpService<DitoMcp, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(DitoMcp { shared: shared.clone() }),
        Arc::default(),
        StreamableHttpServerConfig::default().with_cancellation_token(cancel),
    )
}

/// The input schema of each tool, shared with the endpoint reference dialog.
pub fn tool_schemas() -> [(&'static str, Value); 2] {
    [
        (
            GENERATE_TEXT,
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "The message to answer." },
                    "system": { "type": "string", "description": "Instructions that shape the answer." },
                    "temperature": { "type": "number" },
                    "max_tokens": { "type": "integer", "minimum": 1 },
                    "reasoning_effort": {
                        "type": "string",
                        "description": "How hard the model thinks first."
                    }
                },
                "required": ["prompt"]
            }),
        ),
        (LIST_LOCAL_MODELS, json!({ "type": "object", "properties": {} })),
    ]
}

fn object(value: Value) -> Arc<Map<String, Value>> {
    match value {
        Value::Object(map) => Arc::new(map),
        _ => Arc::default(),
    }
}

fn text_result(text: impl Into<String>, error: bool) -> CallToolResponse {
    let content = vec![ContentBlock::text(text)];
    if error { CallToolResult::error(content) } else { CallToolResult::success(content) }.into()
}

impl ServerHandler for DitoMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_server_info(Implementation::new("dito", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let [(generate_name, generate_schema), (list_name, list_schema)] = tool_schemas();
        Ok(ListToolsResult::with_all_items(vec![
            Tool::new(
                generate_name,
                "Generates text with the local text model Dito is set to.",
                object(generate_schema),
            ),
            Tool::new(
                list_name,
                "Lists the models a client can use and the other installed local models.",
                object(list_schema),
            ),
        ]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let arguments = request.arguments.unwrap_or_default();
        match request.name.as_ref() {
            LIST_LOCAL_MODELS => {
                let listing = model_listing(&self.shared.cfg.get(), self.shared.installed.as_ref());
                Ok(text_result(listing.to_string(), false))
            }
            GENERATE_TEXT => {
                let Some(prompt) = arguments.get("prompt").and_then(Value::as_str) else {
                    return Err(ErrorData::invalid_params("prompt is required and must be a string", None));
                };
                let mut messages = Vec::new();
                if let Some(system) = arguments.get("system").and_then(Value::as_str) {
                    messages.push(ChatMessage::new(ChatRole::System, system));
                }
                messages.push(ChatMessage::new(ChatRole::User, prompt));
                let effort = arguments.get("reasoning_effort").and_then(Value::as_str);
                let thinking = thinking_for(effort, &self.shared.cfg.get(), self.shared.installed.as_ref()).map_err(
                    |supported| {
                        ErrorData::invalid_params(
                            format!("reasoning_effort must be one of: {}", supported.join(", ")),
                            None,
                        )
                    },
                )?;
                let request = ChatRequest {
                    messages,
                    temperature: arguments.get("temperature").and_then(Value::as_f64).map(|t| t as f32),
                    top_p: None,
                    max_tokens: arguments.get("max_tokens").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()),
                    thinking,
                };
                match generate(&self.shared, &request, &ChatCancel::default()).await {
                    Ok(reply) => Ok(text_result(reply, false)),
                    Err(e) => Ok(text_result(e.message, true)),
                }
            }
            other => Err(ErrorData::invalid_params(format!("unknown tool: {other}"), None)),
        }
    }

    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let models = Resource::new(MODELS_RESOURCE, "models")
            .with_description("Model IDs, capabilities and supported formats.")
            .with_mime_type("application/json");
        Ok(ListResourcesResult::with_all_items(vec![models]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<rmcp::RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if request.uri != MODELS_RESOURCE {
            return Err(ErrorData::resource_not_found(format!("unknown resource: {}", request.uri), None));
        }
        let listing = model_listing(&self.shared.cfg.get(), self.shared.installed.as_ref()).to_string();
        let contents = ResourceContents::text(listing, MODELS_RESOURCE);
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}
