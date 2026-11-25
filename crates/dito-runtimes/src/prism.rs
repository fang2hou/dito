//! Managed PrismML `llama-server` worker for ternary PQ2_0 checkpoints until stock `llama-cpp-sys-2`
//! ships the Hadamard activation runtime (`b11556+`).

use std::future::Future;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dito_core::intent::{Reading, Request};
use dito_core::provider::{ChatMessage, ChatRole};
use dito_local::managed_process::ManagedProcess;
use dito_local::{Example, LlmRuntime, RuntimeError, Sampling};
use serde_json::{Number, Value, json};

use crate::qwen_text::{ThinkFilter, strip_think};

const ARCHIVE_SHA: &str = "94d791a4ad9d6ea0c9884929830352397af8274ca107afab497b1de0147bf99b";
const PACKAGE: &str = "llama-prism-b12178-03ec600";
const RUNTIME_DIR: &str = ".prism-llama-b12178-03ec600";
const MODEL_ALIAS: &str = "bonsai";
const POLISH_SEED: u32 = 1234;
const CHAT_BUDGET: u32 = 4096;
const CONTEXT_SIZE: &str = "32768";

pub struct Chat {
    process: ManagedProcess,
    endpoint: String,
    token: String,
}

fn cancelled(stop: &AtomicBool) -> Result<(), RuntimeError> {
    if stop.load(Ordering::Relaxed) { Err(RuntimeError::Cancelled) } else { Ok(()) }
}

fn block_on<T>(future: impl Future<Output = Result<T, RuntimeError>>) -> Result<T, RuntimeError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(RuntimeError::engine)?.block_on(future)
}

fn client(timeout: Duration) -> Result<reqwest::Client, RuntimeError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(timeout)
        .build()
        .map_err(RuntimeError::engine)
}

fn json_number(value: f32) -> Option<Value> {
    value.to_string().parse::<f64>().ok().and_then(Number::from_f64).map(Value::Number)
}

fn install(dir: &Path, archive: &str, stop: &AtomicBool) -> Result<PathBuf, RuntimeError> {
    let lock = std::fs::File::options()
        .create(true)
        .append(true)
        .open(dir.join(".prism-runtime.lock"))
        .map_err(RuntimeError::engine)?;
    loop {
        cancelled(stop)?;
        match lock.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(RuntimeError::engine(error)),
        }
    }
    let destination = dir.join(RUNTIME_DIR);
    let binary = destination.join(PACKAGE).join("llama-server");
    if binary.is_file() {
        return Ok(binary);
    }
    let archive = super::model_file(dir, archive)?;
    let digest = block_on(async { dito_local::download::sha256_file(&archive).await.map_err(RuntimeError::engine) })?;
    if digest != ARCHIVE_SHA {
        return Err(RuntimeError::engine("Prism runtime archive checksum mismatch"));
    }
    cancelled(stop)?;
    let temporary = dir.join(format!("{RUNTIME_DIR}.installing"));
    if temporary.exists() {
        std::fs::remove_dir_all(&temporary).map_err(RuntimeError::engine)?;
    }
    std::fs::create_dir(&temporary).map_err(RuntimeError::engine)?;
    let output = std::process::Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&temporary)
        .output()
        .map_err(RuntimeError::engine)?;
    if !output.status.success() {
        return Err(RuntimeError::engine("could not unpack Prism runtime"));
    }
    if !temporary.join(PACKAGE).join("llama-server").is_file() {
        return Err(RuntimeError::engine("Prism runtime package has no llama-server executable"));
    }
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(RuntimeError::engine)?;
    }
    std::fs::rename(&temporary, &destination).map_err(RuntimeError::engine)?;
    Ok(binary)
}

impl Chat {
    pub fn load(dir: &Path, model: &str, archive: &str, stop: &AtomicBool) -> Result<Self, RuntimeError> {
        cancelled(stop)?;
        let model = super::model_file(dir, model)?;
        let binary = install(dir, archive, stop)?;
        cancelled(stop)?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(RuntimeError::engine)?;
        let port = listener.local_addr().map_err(RuntimeError::engine)?.port();
        let mut secret = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut secret))
            .map_err(RuntimeError::engine)?;
        let token = STANDARD.encode(secret);
        let args: Vec<String> = [
            "-m",
            &model.to_string_lossy(),
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            &token,
            "--alias",
            MODEL_ALIAS,
            "-ngl",
            "99",
            "-fa",
            "on",
            "-c",
            CONTEXT_SIZE,
            "-np",
            "1",
            "--no-webui",
            "--no-warmup",
            "--jinja",
            "--min-p",
            "0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        drop(listener);
        let log = std::fs::File::create(dir.join("prism-llama.log")).map_err(RuntimeError::engine)?;
        let process = ManagedProcess::spawn(&binary, &args, log)?;
        let engine = Self { process, endpoint: format!("http://127.0.0.1:{port}"), token };
        block_on(async {
            let client = client(Duration::from_secs(2))?;
            let deadline = Instant::now() + Duration::from_secs(180);
            loop {
                cancelled(stop)?;
                if let Some(error) = engine.failure() {
                    return Err(RuntimeError::engine(format!("{error}; see prism-llama.log")));
                }
                if Instant::now() >= deadline {
                    return Err(RuntimeError::engine("Prism model startup timed out; see prism-llama.log"));
                }
                let probe = client.get(format!("{}/health", engine.endpoint)).bearer_auth(&engine.token).send();
                tokio::select! {
                    response = probe => {
                        if let Ok(response) = response && response.status().is_success() {
                            return Ok(());
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })?;
        cancelled(stop)?;
        Ok(engine)
    }

    async fn post_json(
        &self,
        client: &reqwest::Client,
        body: &Value,
        stop: &AtomicBool,
    ) -> Result<Value, RuntimeError> {
        let request = async {
            let response = client
                .post(format!("{}/v1/chat/completions", self.endpoint))
                .bearer_auth(&self.token)
                .json(body)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(RuntimeError::engine)?;
            response.json::<Value>().await.map_err(RuntimeError::engine)
        };
        tokio::pin!(request);
        loop {
            cancelled(stop)?;
            if let Some(error) = self.failure() {
                return Err(RuntimeError::engine(error));
            }
            tokio::select! {
                value = &mut request => return value,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
    }
}

fn polish_budget(system: &str, examples: &[Example], user: &str, cap: Option<u32>) -> u32 {
    let chars = system.chars().count()
        + user.chars().count()
        + examples.iter().map(|e| e.said.chars().count() + e.written.chars().count()).sum::<usize>();
    let approx_tokens = (chars / 2).max(1) as u32;
    let computed = approx_tokens.saturating_mul(3).saturating_div(2).clamp(256, 4096);
    cap.map_or(computed, |cap| computed.min(cap))
}

fn apply_sampling(body: &mut Value, sampling: &Sampling, max_tokens: u32) {
    body["max_tokens"] = max_tokens.into();
    body["min_p"] = json!(0.0);
    if let Some(temp) = json_number(sampling.temperature) {
        body["temperature"] = temp;
    }
    if let Some(top_p) = sampling.top_p.and_then(json_number) {
        body["top_p"] = top_p;
    }
    if let Some(top_k) = sampling.top_k {
        body["top_k"] = top_k.into();
    }
}

/// `LocalLlm::chat` already prepends the `xhigh` system instruction when `xhigh` is chosen and adds
/// nothing for `medium`. Passing `reasoning_effort: "medium"` to the Jinja template keeps thinking
/// open without duplicating the `xhigh` line.
fn thinking_kwargs(think: Option<bool>) -> Value {
    match think {
        Some(true) => json!({ "enable_thinking": true, "reasoning_effort": "medium" }),
        Some(false) | None => json!({ "enable_thinking": false }),
    }
}

#[derive(Default)]
struct SseLines {
    pending: Vec<u8>,
}

impl SseLines {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut payloads = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            if let Some(data) = line.trim_end_matches(['\r', '\n']).strip_prefix("data:") {
                payloads.push(data.strip_prefix(' ').unwrap_or(data).to_string());
            }
        }
        payloads
    }
}

enum StreamFrame {
    Content(String),
    Reasoning,
    Done,
    Skip,
}

fn parse_stream_frame(data: &str) -> Result<StreamFrame, RuntimeError> {
    let data = data.trim();
    if data.is_empty() {
        return Ok(StreamFrame::Skip);
    }
    if data == "[DONE]" {
        return Ok(StreamFrame::Done);
    }
    let value: Value = serde_json::from_str(data).map_err(RuntimeError::engine)?;
    if let Some(error) = value.get("error").filter(|e| !e.is_null()) {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("stream error");
        return Err(RuntimeError::engine(message));
    }
    if let Some(text) = value.pointer("/choices/0/delta/content").and_then(Value::as_str)
        && !text.is_empty()
    {
        return Ok(StreamFrame::Content(text.to_owned()));
    }
    if value.pointer("/choices/0/delta/reasoning_content").and_then(Value::as_str).is_some_and(|text| !text.is_empty())
    {
        return Ok(StreamFrame::Reasoning);
    }
    Ok(StreamFrame::Skip)
}

fn parse_option_logprobs(value: &Value, options: &[&str]) -> Result<Vec<f32>, RuntimeError> {
    let entries = value
        .pointer("/choices/0/logprobs/content/0/top_logprobs")
        .and_then(Value::as_array)
        .filter(|arr| !arr.is_empty())
        .ok_or_else(|| RuntimeError::engine("Prism runtime returned no option logprobs"))?;
    let mut min_logprob = f32::INFINITY;
    let mut parsed: Vec<(&str, f32)> = Vec::with_capacity(entries.len());
    for entry in entries {
        if let (Some(token), Some(lp)) =
            (entry.get("token").and_then(Value::as_str), entry.get("logprob").and_then(Value::as_f64))
        {
            let lp = lp as f32;
            min_logprob = min_logprob.min(lp);
            parsed.push((token, lp));
        }
    }
    if !min_logprob.is_finite() {
        return Err(RuntimeError::engine("Prism runtime returned no usable option logprobs"));
    }
    let floor = min_logprob - 20.0;
    let logits = options
        .iter()
        .map(|option| {
            parsed
                .iter()
                .find(|(token, _)| token == option)
                .or_else(|| parsed.iter().find(|(token, _)| token.trim() == *option))
                .map_or(floor, |(_, lp)| *lp)
        })
        .collect();
    Ok(logits)
}

impl LlmRuntime for Chat {
    fn shutdown(&self) {
        self.process.shutdown();
    }

    fn failure(&self) -> Option<String> {
        self.process.failure()
    }

    fn process_id(&self) -> Option<u32> {
        Some(self.process.pid())
    }

    fn generate(
        &self,
        system: &str,
        examples: &[Example],
        user: &str,
        sampling: &Sampling,
        stop: &AtomicBool,
    ) -> Result<String, RuntimeError> {
        cancelled(stop)?;
        if let Some(error) = self.failure() {
            return Err(RuntimeError::engine(error));
        }
        let mut messages = Vec::with_capacity(examples.len() * 2 + 2);
        messages.push(json!({ "role": "system", "content": system }));
        for example in examples {
            messages.push(json!({ "role": "user", "content": example.said }));
            messages.push(json!({ "role": "assistant", "content": example.written }));
        }
        messages.push(json!({ "role": "user", "content": user }));
        let mut body = json!({
            "model": MODEL_ALIAS,
            "messages": messages,
            "stream": false,
            "seed": POLISH_SEED,
            "chat_template_kwargs": thinking_kwargs(Some(false)),
        });
        apply_sampling(&mut body, sampling, polish_budget(system, examples, user, sampling.max_tokens));
        let value = block_on(async {
            let client = client(Duration::from_secs(300))?;
            self.post_json(&client, &body, stop).await
        })?;
        cancelled(stop)?;
        if value.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("length") {
            return Err(RuntimeError::Truncated);
        }
        let output = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::engine("Prism runtime returned no content"))?;
        Ok(strip_think(output))
    }

    fn chat(
        &self,
        messages: &[ChatMessage],
        sampling: &Sampling,
        think: Option<bool>,
        on_piece: &mut dyn FnMut(&str),
        stop: &AtomicBool,
    ) -> Result<String, RuntimeError> {
        if stop.load(Ordering::Relaxed) {
            return Ok(String::new());
        }
        if let Some(error) = self.failure() {
            return Err(RuntimeError::engine(error));
        }
        let wire_messages: Vec<Value> = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    ChatRole::System => "system",
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                };
                json!({ "role": role, "content": m.content })
            })
            .collect();
        let mut body = json!({
            "model": MODEL_ALIAS,
            "messages": wire_messages,
            "stream": true,
            "chat_template_kwargs": thinking_kwargs(think),
        });
        apply_sampling(&mut body, sampling, sampling.max_tokens.unwrap_or(CHAT_BUDGET));

        let mut filter = ThinkFilter::new(false);
        let mut reply = String::new();
        let mut saw_reasoning = false;
        let mut show = |text: &str| {
            if !text.is_empty() {
                on_piece(text);
                reply.push_str(text);
            }
        };

        let cancelled_during_stream = block_on(async {
            let client = client(Duration::from_secs(600))?;
            let open = async {
                client
                    .post(format!("{}/v1/chat/completions", self.endpoint))
                    .bearer_auth(&self.token)
                    .json(&body)
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .map_err(RuntimeError::engine)
            };
            tokio::pin!(open);
            let mut response = loop {
                if stop.load(Ordering::Relaxed) {
                    return Ok(true);
                }
                if let Some(error) = self.failure() {
                    return Err(RuntimeError::engine(error));
                }
                tokio::select! {
                    resp = &mut open => break resp?,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            };
            let mut lines = SseLines::default();
            loop {
                if stop.load(Ordering::Relaxed) {
                    return Ok(true);
                }
                let next_chunk = response.chunk();
                tokio::pin!(next_chunk);
                let chunk = loop {
                    if stop.load(Ordering::Relaxed) {
                        return Ok(true);
                    }
                    if let Some(error) = self.failure() {
                        return Err(RuntimeError::engine(error));
                    }
                    tokio::select! {
                        chunk = &mut next_chunk => break chunk.map_err(RuntimeError::engine)?,
                        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                    }
                };
                let Some(chunk) = chunk else { break };
                for data in lines.push(&chunk) {
                    match parse_stream_frame(&data)? {
                        StreamFrame::Content(piece) => show(&filter.push(&piece)),
                        StreamFrame::Reasoning => saw_reasoning = true,
                        StreamFrame::Done => return Ok(false),
                        StreamFrame::Skip => {}
                    }
                    if stop.load(Ordering::Relaxed) {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        })?;

        if cancelled_during_stream {
            return Ok(reply);
        }
        let tail = filter.finish();
        if think == Some(true) {
            if reply.is_empty() && (saw_reasoning || !tail.is_empty()) {
                return Err(RuntimeError::engine("the model used its whole reply on thinking and never answered"));
            }
        } else {
            show(&tail);
        }
        Ok(reply)
    }

    fn read(&self, request: &Request, stop: &AtomicBool) -> Result<Reading, RuntimeError> {
        if !request.parts.is_empty() {
            return Err(RuntimeError::ReadUnsupported);
        }
        cancelled(stop)?;
        if let Some(error) = self.failure() {
            return Err(RuntimeError::engine(error));
        }
        let body = json!({
            "model": MODEL_ALIAS,
            "messages": [
                { "role": "system", "content": request.system },
                { "role": "user", "content": request.user },
            ],
            "stream": false,
            "max_tokens": 1,
            "temperature": 0.0,
            "min_p": 0.0,
            "logprobs": true,
            "top_logprobs": 20,
            "chat_template_kwargs": thinking_kwargs(Some(false)),
        });
        let value = block_on(async {
            let client = client(Duration::from_secs(60))?;
            self.post_json(&client, &body, stop).await
        })?;
        cancelled(stop)?;
        let option_logits = parse_option_logprobs(&value, request.options)?;
        Ok(Reading { option_logits, features: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_kwargs_disable_by_default_and_prevent_double_xhigh_prompt() {
        assert_eq!(thinking_kwargs(None), json!({ "enable_thinking": false }));
        assert_eq!(thinking_kwargs(Some(false)), json!({ "enable_thinking": false }));
        assert_eq!(thinking_kwargs(Some(true)), json!({ "enable_thinking": true, "reasoning_effort": "medium" }));
    }

    #[test]
    fn sse_lines_and_frames_separate_reasoning_and_content() {
        let mut sse = SseLines::default();
        let payloads = sse.push(
            concat!(
                "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"hmm\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n",
                "data: [DONE]\n"
            )
            .as_bytes(),
        );
        assert_eq!(payloads.len(), 3);
        assert!(matches!(parse_stream_frame(&payloads[0]).unwrap(), StreamFrame::Reasoning));
        assert!(matches!(parse_stream_frame(&payloads[1]).unwrap(), StreamFrame::Content(ref s) if s == "hello"));
        assert!(matches!(parse_stream_frame(&payloads[2]).unwrap(), StreamFrame::Done));
    }

    #[test]
    fn option_logprobs_preserve_order_and_floor_missing_options() {
        let response = json!({
            "choices": [{
                "logprobs": {
                    "content": [{
                        "top_logprobs": [
                            { "token": "B", "logprob": -0.2 },
                            { "token": " A", "logprob": -1.5 }
                        ]
                    }]
                }
            }]
        });
        let logits = parse_option_logprobs(&response, &["A", "B", "C"]).unwrap();
        assert!((logits[0] - (-1.5)).abs() < 1e-5);
        assert!((logits[1] - (-0.2)).abs() < 1e-5);
        assert!((logits[2] - (-21.5)).abs() < 1e-5);
    }
}
