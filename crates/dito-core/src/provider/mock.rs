//! Deterministic in-process engines for `--mock` runs and tests. No network, no model.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use super::{AsrRequest, PolishRequest, Polisher, Transcriber, Transcript};
use crate::error::{ErrorKind, ProviderError, ProviderResult};

/// One scripted reply: text, or a failure class.
pub type Reply = Result<String, ErrorKind>;

pub struct MockTranscriber {
    script: Mutex<VecDeque<Reply>>,
    fallback: String,
    delay: Duration,
    calls: AtomicUsize,
}

impl MockTranscriber {
    pub fn new(fallback: impl Into<String>) -> Self {
        Self { script: Mutex::default(), fallback: fallback.into(), delay: Duration::ZERO, calls: AtomicUsize::new(0) }
    }

    pub fn with_delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }

    /// Queue replies consumed one per call before the fallback is used.
    pub fn script(self, replies: impl IntoIterator<Item = Reply>) -> Self {
        self.script.lock().expect("script mutex poisoned").extend(replies);
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Transcriber for MockTranscriber {
    async fn transcribe(&self, _req: &AsrRequest<'_>) -> ProviderResult<Transcript> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        let next = self.script.lock().expect("script mutex poisoned").pop_front();
        match next.unwrap_or_else(|| Ok(self.fallback.clone())) {
            Ok(text) => Ok(Transcript { text, language: None }),
            Err(kind) => Err(ProviderError::new(kind, "scripted failure")),
        }
    }
}

pub struct MockPolisher {
    script: Mutex<VecDeque<Reply>>,
    delay: Duration,
    calls: AtomicUsize,
}

impl Default for MockPolisher {
    fn default() -> Self {
        Self { script: Mutex::default(), delay: Duration::ZERO, calls: AtomicUsize::new(0) }
    }
}

const FILLERS: &[&str] = &["えーと", "えっと", "あの", "その", "まあ", "um", "uh", "嗯", "那个"];

/// Rule-based stand-in for the model: strips fillers and normalizes spacing and the final period.
pub fn rule_polish(text: &str) -> String {
    let mut out = text.to_string();
    for f in FILLERS {
        out = out.replace(f, "");
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let out = out.trim_matches(|c: char| c == ',' || c == '、' || c.is_whitespace()).to_string();
    let is_cjk = out.chars().any(|c| ('\u{3040}'..='\u{9fff}').contains(&c));
    match (out.chars().last(), is_cjk) {
        (None, _) => out,
        (Some('。' | '.' | '！' | '？' | '!' | '?'), _) => out,
        (_, true) => format!("{out}。"),
        (_, false) => format!("{out}."),
    }
}

impl MockPolisher {
    pub fn with_delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }

    pub fn script(self, replies: impl IntoIterator<Item = Reply>) -> Self {
        self.script.lock().expect("script mutex poisoned").extend(replies);
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Polisher for MockPolisher {
    async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        match self.script.lock().expect("script mutex poisoned").pop_front() {
            Some(Ok(text)) => Ok(text),
            Some(Err(kind)) => Err(ProviderError::new(kind, "scripted failure")),
            None => Ok(rule_polish(req.text)),
        }
    }
}
