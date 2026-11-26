//! The one task that runs speech recognition for a session. Finished utterances come first and in order; the
//! utterance still going on is recognized only while nothing finished waits, newest audio only, and a partial in
//! progress is cancelled the moment a finished utterance arrives, so the partial text never delays the final one.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use super::Speaker;
use crate::audio::AudioClip;
use crate::error::ErrorKind;
use crate::i18n::Lang;
use crate::provider::{AsrRequest, Transcriber};
use crate::spoken_forms::SpokenForms;

pub(super) struct Utterance {
    pub speaker: Speaker,
    pub seq: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub samples: Vec<f32>,
}

pub(super) enum Job {
    Final(Utterance),
    /// The utterance `seq` so far.
    Partial {
        speaker: Speaker,
        seq: u64,
        start_ms: u64,
        samples: Vec<f32>,
    },
}

pub(super) enum Recognized {
    /// `text` is `None` for an utterance with nothing recognizable or a failed recognition. `lang` is the language
    /// the engine reported, which the paragraph's spoken forms are matched in.
    Final {
        speaker: Speaker,
        seq: u64,
        start_ms: u64,
        end_ms: u64,
        text: Option<String>,
        lang: Option<Lang>,
    },
    Partial {
        speaker: Speaker,
        seq: u64,
        start_ms: u64,
        text: String,
    },
    /// Recognition of a finished utterance failed; the session goes on.
    Failed(String),
}

pub(super) struct Recognizer {
    pub asr: Arc<dyn Transcriber>,
    pub language: Option<String>,
    /// The terms of the wordbook, passed to ASR as hotwords.
    pub words: Vec<String>,
    /// The spoken forms of the wordbook, compiled once for the whole session: this task runs for every partial.
    pub forms: Arc<SpokenForms>,
}

struct Partial {
    speaker: Speaker,
    seq: u64,
    start_ms: u64,
    samples: Vec<f32>,
}

impl Recognizer {
    /// Runs until `jobs` closes and every finished utterance has been recognized. Partial audio left at that point is
    /// dropped: the session is finishing and only final text counts.
    pub async fn run(self, mut jobs: mpsc::UnboundedReceiver<Job>, out: mpsc::UnboundedSender<Recognized>) {
        let mut finals: VecDeque<Utterance> = VecDeque::new();
        // The newest audio per speaker, oldest request first so neither speaker starves.
        let mut partials: VecDeque<Partial> = VecDeque::new();
        let mut open = true;
        loop {
            while open {
                match jobs.try_recv() {
                    Ok(job) => queue(job, &mut finals, &mut partials),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => open = false,
                }
            }
            if let Some(u) = finals.pop_front() {
                let started = Instant::now();
                let (speaker, seq, start_ms, end_ms) = (u.speaker, u.seq, u.start_ms, u.end_ms);
                let secs = u.samples.len() as f32 / crate::audio::SAMPLE_RATE as f32;
                let (text, lang) = match self.transcribe(u.samples).await {
                    Ok(Some((text, lang))) => (Some(text), lang),
                    Ok(None) => (None, None),
                    Err(e) => {
                        tracing::warn!("transcription of an utterance failed: {e}");
                        let _ = out.send(Recognized::Failed(e));
                        (None, None)
                    }
                };
                tracing::debug!("final {speaker:?}#{seq} ({secs:.1} s of audio) in {:?}", started.elapsed());
                let _ = out.send(Recognized::Final { speaker, seq, start_ms, end_ms, text, lang });
                continue;
            }
            if !open {
                return;
            }
            let Some(p) = partials.pop_front() else {
                match jobs.recv().await {
                    Some(job) => queue(job, &mut finals, &mut partials),
                    None => open = false,
                }
                continue;
            };
            let (speaker, seq, start_ms) = (p.speaker, p.seq, p.start_ms);
            let started = Instant::now();
            let work = self.transcribe(p.samples);
            tokio::pin!(work);
            // Dropping `work` cancels the recognition at its next token.
            let text = loop {
                tokio::select! {
                    text = &mut work => break text.ok().flatten().map(|(text, _)| text),
                    job = jobs.recv() => match job {
                        Some(Job::Final(u)) => {
                            queue(Job::Final(u), &mut finals, &mut partials);
                            tracing::debug!("partial {speaker:?}#{seq} cancelled for a final");
                            break None;
                        }
                        Some(job) => queue(job, &mut finals, &mut partials),
                        None => {
                            open = false;
                            break None;
                        }
                    },
                }
            };
            if let Some(text) = text {
                tracing::debug!("partial {speaker:?}#{seq} in {:?}", started.elapsed());
                let _ = out.send(Recognized::Partial { speaker, seq, start_ms, text });
            }
        }
    }

    /// The text of the utterance with each spoken form written as its term, and the language it was matched in.
    async fn transcribe(&self, samples: Vec<f32>) -> Result<Option<(String, Option<Lang>)>, String> {
        let clip = AudioClip::new(samples);
        let request = AsrRequest { clip: &clip, language: self.language.as_deref(), hotwords: &self.words };
        match self.asr.transcribe(&request).await {
            Ok(t) => {
                let raw = t.text.trim();
                let lang = crate::pipeline::spoken_forms_lang(t.language.as_deref(), raw);
                let text = self.forms.apply_for_lang(raw, lang).into_owned();
                Ok(Some((text, lang)).filter(|(text, _)| !text.is_empty()))
            }
            Err(e) if e.kind == ErrorKind::NoSpeech => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// A finished utterance makes its speaker's waiting partial audio stale; newer partial audio replaces older.
fn queue(job: Job, finals: &mut VecDeque<Utterance>, partials: &mut VecDeque<Partial>) {
    match job {
        Job::Final(u) => {
            partials.retain(|p| !(p.speaker == u.speaker && p.seq <= u.seq));
            finals.push_back(u);
        }
        Job::Partial { speaker, seq, start_ms, samples } => {
            if finals.iter().any(|u| u.speaker == speaker && u.seq >= seq) {
                return;
            }
            match partials.iter_mut().find(|p| p.speaker == speaker) {
                Some(p) => *p = Partial { speaker, seq, start_ms, samples },
                None => partials.push_back(Partial { speaker, seq, start_ms, samples }),
            }
        }
    }
}
