//! Deterministic in-process engines for `--mock` runs and tests. No network, no model.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use super::{AsrRequest, PolishRequest, Polisher, Router, Transcriber, Transcript};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::intent::{Decision, Intent};

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

/// Takes the user turn of a polish request: the data of a command tool, or a dictation transcript, fenced or not.
pub fn rule_polish(text: &str) -> String {
    let mut out = crate::command::turn_data(text).unwrap_or_else(|| crate::prompts::unfenced(text).to_string());
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

/// Routes every instruction to `fallback` unless decisions are queued.
pub struct MockRouter {
    script: Mutex<VecDeque<Result<Decision, ErrorKind>>>,
    fallback: Decision,
    calls: AtomicUsize,
}

impl Default for MockRouter {
    fn default() -> Self {
        Self::new(decision(Intent::Polish, 0.99))
    }
}

/// `confidence` goes to `intent` and the rest is shared evenly, so `ranked` lists the other intents in option order.
pub fn decision(intent: Intent, confidence: f32) -> Decision {
    let rest = (1. - confidence) / (Intent::COUNT - 1) as f32;
    let probabilities = Intent::ALL.map(|i| if i == intent { confidence } else { rest });
    Decision { intent, confidence, probabilities, rewrite_op: None }
}

impl MockRouter {
    pub fn new(fallback: Decision) -> Self {
        Self { script: Mutex::default(), fallback, calls: AtomicUsize::new(0) }
    }

    /// Queue replies consumed one per call before the fallback is used.
    pub fn script(self, replies: impl IntoIterator<Item = Result<Decision, ErrorKind>>) -> Self {
        self.script.lock().expect("script mutex poisoned").extend(replies);
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Router for MockRouter {
    async fn decide(&self, _instruction: &str) -> ProviderResult<Decision> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.script.lock().expect("script mutex poisoned").pop_front() {
            Some(Ok(decision)) => Ok(decision),
            Some(Err(kind)) => Err(ProviderError::new(kind, "scripted failure")),
            None => Ok(self.fallback.clone()),
        }
    }
}
