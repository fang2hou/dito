//! Azure OpenAI and OpenAI-compatible chat engines for polishing (one wire format, two URL/auth schemes).

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{PolishRequest, Polisher, default_timeout};
use crate::config::ReasoningEffort;
use crate::error::{ErrorKind, ProviderError, ProviderResult};

#[derive(Debug, Clone)]
pub enum Flavor {
    /// `{endpoint}/openai/v1/...`, `api-key` header, `model` = deployment name.
    Azure { endpoint: String, deployment: String },
    /// `{base_url}/...`, optional `Authorization: Bearer`.
    Compat { base_url: String, model: String },
}

#[derive(Clone)]
pub struct OpenAiEngine {
    flavor: Flavor,
    api_key: String,
    effort: ReasoningEffort,
    client: reqwest::Client,
}

fn trim_known_suffix(url: &str) -> &str {
    let u = url.trim().trim_end_matches('/');
    u.strip_suffix("/chat/completions").unwrap_or(u)
}

impl OpenAiEngine {
    pub fn new(flavor: Flavor, api_key: impl Into<String>, effort: ReasoningEffort) -> ProviderResult<Self> {
        let (name, value) = match &flavor {
            Flavor::Azure { endpoint, deployment } => ("endpoint / deployment", (endpoint.trim(), deployment.trim())),
            Flavor::Compat { base_url, model } => ("base URL / model", (base_url.trim(), model.trim())),
        };
        if value.0.is_empty() || value.1.is_empty() {
            return Err(ProviderError::new(ErrorKind::Unavailable, format!("{name} is not set")));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(default_timeout())
            .build()
            .map_err(|e| ProviderError::new(ErrorKind::Other, e.to_string()))?;
        Ok(Self { flavor, api_key: api_key.into(), effort, client })
    }

    fn url(&self, path: &str) -> String {
        match &self.flavor {
            Flavor::Azure { endpoint, .. } => format!("{}/openai/v1/{path}", endpoint.trim().trim_end_matches('/')),
            Flavor::Compat { base_url, .. } => format!("{}/{path}", trim_known_suffix(base_url)),
        }
    }

    fn model(&self) -> &str {
        match &self.flavor {
            Flavor::Azure { deployment, .. } => deployment.trim(),
            Flavor::Compat { model, .. } => model.trim(),
        }
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.api_key.is_empty() {
            return rb;
        }
        match self.flavor {
            Flavor::Azure { .. } => rb.header("api-key", &self.api_key),
            Flavor::Compat { .. } => rb.bearer_auth(&self.api_key),
        }
    }

    async fn send_json(&self, rb: reqwest::RequestBuilder) -> ProviderResult<Value> {
        let resp = rb.send().await.map_err(|e| ProviderError::from_reqwest(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ProviderError::from_reqwest(&e))?;
        if !status.is_success() {
            return Err(ProviderError::from_status(status.as_u16(), &body));
        }
        serde_json::from_str(&body)
            .map_err(|e| ProviderError::new(ErrorKind::BadResponse, format!("invalid JSON: {e}")))
    }

    async fn chat(&self, body: &Value) -> ProviderResult<Value> {
        self.send_json(self.auth(self.client.post(self.url("chat/completions"))).json(body)).await
    }
}

#[async_trait]
impl Polisher for OpenAiEngine {
    async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String> {
        let mut body = json!({
            "model": self.model(),
            "messages": [
                { "role": "system", "content": req.system },
                { "role": "user", "content": req.text },
            ],
        });
        if !self.effort.as_str().trim().is_empty() {
            body["reasoning_effort"] = self.effort.as_str().into();
        }
        let v = match self.chat(&body).await {
            // Non-reasoning models reject the field; retry once without it.
            Err(e) if e.kind == ErrorKind::Other && e.message.contains("reasoning_effort") => {
                body.as_object_mut().expect("chat body is a JSON object").remove("reasoning_effort");
                self.chat(&body).await?
            }
            other => other?,
        };
        if v.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("length") {
            return Err(ProviderError::new(ErrorKind::BadResponse, "output was cut off at the token limit"));
        }
        let text = v
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::new(ErrorKind::BadResponse, "no content in response"))?;
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compat(base: &str) -> OpenAiEngine {
        OpenAiEngine::new(Flavor::Compat { base_url: base.into(), model: "m".into() }, "", ReasoningEffort::Low)
            .unwrap()
    }

    #[test]
    fn compat_urls_tolerate_pasted_full_paths() {
        for base in ["http://h/v1", "http://h/v1/", "http://h/v1/chat/completions", "http://h/v1/chat/completions/"] {
            assert_eq!(compat(base).url("chat/completions"), "http://h/v1/chat/completions", "{base}");
        }
    }

    #[test]
    fn azure_urls_use_the_v1_surface() {
        let e = OpenAiEngine::new(
            Flavor::Azure { endpoint: "https://x.openai.azure.com/".into(), deployment: "gpt-6.1-terra".into() },
            "k",
            ReasoningEffort::Low,
        )
        .unwrap();
        assert_eq!(e.url("chat/completions"), "https://x.openai.azure.com/openai/v1/chat/completions");
        assert_eq!(e.model(), "gpt-6.1-terra");
    }
}
