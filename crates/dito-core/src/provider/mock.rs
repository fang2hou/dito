//! Deterministic in-process engines for `--mock` runs and tests. No network, no model.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use super::{
    AsrRequest, ChatCancel, ChatDelta, ChatRequest, ChatRole, Router, TextModel, TextRequest, Transcriber, Transcript,
};
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

pub struct MockTextModel {
    script: Mutex<VecDeque<ProviderResult<String>>>,
    delay: Duration,
    calls: AtomicUsize,
}

impl Default for MockTextModel {
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

/// What a mock engine answers to any text model request: the name-shaped words of a word finder passage, one per
/// line, or the rule-polished text of everything else.
pub fn rule_reply(system: &str, user: &str) -> String {
    if !crate::word_finder::is_request(system) {
        return rule_polish(user);
    }
    let terms: Vec<&str> = crate::prompts::unfenced(user)
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .map(|word| word.trim_matches(['-', '_', '.']))
        .filter(|word| word.len() > 1 && crate::word_finder::is_name_shaped(word))
        .collect();
    if terms.is_empty() { "NONE".into() } else { terms.join("\n") }
}

impl MockTextModel {
    pub fn with_delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }

    pub fn script(self, replies: impl IntoIterator<Item = Reply>) -> Self {
        self.script.lock().expect("script mutex poisoned").extend(
            replies.into_iter().map(|reply| reply.map_err(|kind| ProviderError::new(kind, "scripted failure"))),
        );
        self
    }

    pub fn script_provider(self, replies: impl IntoIterator<Item = ProviderResult<String>>) -> Self {
        self.script.lock().expect("script mutex poisoned").extend(replies);
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl TextModel for MockTextModel {
    async fn polish(&self, req: &TextRequest<'_>) -> ProviderResult<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        self.script
            .lock()
            .expect("script mutex poisoned")
            .pop_front()
            .unwrap_or_else(|| Ok(rule_reply(req.system, req.text)))
    }

    /// Streams the next scripted reply, or the rule reply to the last user turn, a few characters at a time.
    async fn chat(&self, req: &ChatRequest, on_delta: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        let last = |role: ChatRole| req.messages.iter().rfind(|m| m.role == role).map_or("", |m| m.content.as_str());
        let reply = match self.script.lock().expect("script mutex poisoned").pop_front() {
            Some(reply) => reply?,
            None => rule_reply(last(ChatRole::System), last(ChatRole::User)),
        };
        let chars: Vec<char> = reply.chars().collect();
        let mut sent = String::new();
        for (i, piece) in chars.chunks(CHAT_PIECE_CHARS).enumerate() {
            if cancel.is_cancelled() {
                break;
            }
            if i > 0 {
                tokio::time::sleep(CHAT_PIECE_DELAY).await;
                if cancel.is_cancelled() {
                    break;
                }
            }
            let piece: String = piece.iter().collect();
            on_delta(&piece);
            sent.push_str(&piece);
        }
        Ok(sent)
    }
}

const CHAT_PIECE_CHARS: usize = 3;
const CHAT_PIECE_DELAY: Duration = Duration::from_millis(40);

/// Recognizes scripted speech: line `i` of the script is spoken as a tone of [`tone_hz`]`(i)`, and the text of a clip
/// is as much of that line as the clip holds tone, so partial audio reads as a partial line. Takes longer for longer
/// clips, like a real model, and stops early when the request is dropped.
pub struct ToneTranscriber {
    /// (text, seconds the line is spoken for)
    lines: Vec<(String, f32)>,
    base: Duration,
    per_second: Duration,
}

const TONE_RATE: f32 = 16_000.;
const TONE_LEVEL: f32 = 0.3;

pub fn tone_hz(line: usize) -> f32 {
    200. + 50. * line as f32
}

/// `secs` of line `line` spoken, as 16 kHz samples.
pub fn tone(line: usize, secs: f32) -> Vec<f32> {
    let step = std::f32::consts::TAU * tone_hz(line) / TONE_RATE;
    (0..(secs * TONE_RATE) as usize).map(|i| (i as f32 * step).sin() * TONE_LEVEL).collect()
}

impl ToneTranscriber {
    pub fn new(lines: Vec<(String, f32)>, base: Duration, per_second: Duration) -> Self {
        Self { lines, base, per_second }
    }
}

#[async_trait]
impl Transcriber for ToneTranscriber {
    async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript> {
        let samples = &req.clip.samples;
        tokio::time::sleep(self.base + self.per_second.mul_f32(samples.len() as f32 / TONE_RATE)).await;
        // The tone runs from the first loud sample to the last; each period crosses zero twice.
        let first = samples.iter().position(|s| s.abs() > 0.01).unwrap_or(0);
        let last = samples.iter().rposition(|s| s.abs() > 0.01).unwrap_or(0);
        let tone = &samples[first..=last.max(first)];
        let secs = tone.len() as f32 / TONE_RATE;
        if secs < 0.2 {
            return Err(ProviderError::new(ErrorKind::NoSpeech, "no tone"));
        }
        let crossings = tone.windows(2).filter(|w| (w[0] < 0.) != (w[1] < 0.)).count() as f32;
        let hz = crossings / 2. / secs;
        let line = ((hz - tone_hz(0)) / (tone_hz(1) - tone_hz(0))).round().max(0.) as usize;
        let Some((text, spoken)) = self.lines.get(line) else {
            return Err(ProviderError::new(ErrorKind::NoSpeech, "unknown tone"));
        };
        let chars = text.chars().count();
        let heard = ((secs / spoken).min(1.) * chars as f32).ceil() as usize;
        Ok(Transcript { text: text.chars().take(heard).collect(), language: None })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ChatMessage;

    fn ask(text: &str) -> ChatRequest {
        ChatRequest { messages: vec![ChatMessage::new(ChatRole::User, text)], ..ChatRequest::default() }
    }

    #[tokio::test(start_paused = true)]
    async fn a_mock_chat_streams_several_pieces_and_stops_when_cancelled() {
        let model = MockTextModel::default().script([Ok("one two three four".to_string())]);
        let mut pieces = Vec::new();
        let reply = model.chat(&ask("hi"), &mut |p| pieces.push(p.to_string()), &ChatCancel::default()).await.unwrap();
        assert_eq!((reply.as_str(), pieces.concat()), ("one two three four", reply.clone()));
        assert!(pieces.len() > 3);

        let cancel = ChatCancel::default();
        let mut first = Vec::new();
        let partial = model
            .chat(
                &ask("um a long enough reply"),
                &mut |p| {
                    first.push(p.to_string());
                    cancel.cancel();
                },
                &cancel,
            )
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(partial, first[0]);
        assert_eq!(model.calls(), 2);
    }
}
