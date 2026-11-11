use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::command::Command;
use super::env::{AppInfo, Delivery, Deps, MuteSession};
use super::handle::{DictationHandle, Msg};
use super::history::History;
use super::hud::{ErrorAction, HudState};
use super::progress::{Models, ProcessingEstimate, TimingHistory};
use crate::audio::AudioClip;
use crate::config::{EngineLabels, MusicMuteMode, RecordMode};
use crate::error::{ErrorKind, ProviderError};
use crate::pipeline::{self, Output, PipelineError, PipelineJob, PipelinePhase};
use crate::prompts::Style;
use crate::store::{HistoryKind, NewHistory, Store};

/// Maximum duration of a single recording.
pub const MAX_RECORDING_DURATION: Duration = Duration::from_secs(5 * 60);

/// How long the HUD's whole-capsule outcome animation runs; the HUD is hidden once it has finished.
const SUCCESS_FLASH: Duration = Duration::from_millis(1430);
const COPIED_FLASH: Duration = Duration::from_millis(1930);
const ERROR_LINGER: Duration = Duration::from_secs(3);

struct Meta {
    app: AppInfo,
    style: Style,
    ts: i64,
}

enum State {
    Idle,
    Listening {
        meta: Meta,
        /// `Some` while recording has muted other audio; dropped when the microphone stops.
        mute: Option<Box<dyn MuteSession>>,
    },
    Processing {
        meta: Meta,
        clip: Arc<AudioClip>,
        cancel: CancellationToken,
        models: Models,
        /// Labels of the engines in the settings snapshot this run uses. Every history row the run ends in records
        /// them; a retry takes a new snapshot and its own labels.
        labels: EngineLabels,
    },
    /// The text is being typed on the blocking pool and only `Msg::Delivered` leaves this state. Insertion cannot be
    /// cancelled, so commands are ignored except a key press, which waits for the outcome.
    Delivering {
        /// The shortcut went down meanwhile; in hold mode, releasing it again clears this.
        key_down: bool,
    },
    /// Terminal HUD state being shown (success flash or error). `clip` allows retry.
    Showing {
        clip: Option<(Arc<AudioClip>, Meta)>,
    },
}

struct Actor {
    deps: Deps,
    history: History,
    hud: watch::Sender<HudState>,
    tx: mpsc::UnboundedSender<Msg>,
    state: State,
    /// Bumped on every transition so late jobs and timers are ignored.
    epoch: u64,
    recording_started: watch::Sender<Option<Instant>>,
    processing: watch::Sender<ProcessingEstimate>,
    timings: TimingHistory,
    hide_generation: u64,
    hide_paused: bool,
}

pub fn spawn(deps: Deps) -> DictationHandle {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (hud_tx, hud_rx) = watch::channel(HudState::Hidden);
    let (started_tx, started_rx) = watch::channel(None);
    let (processing_tx, processing_rx) = watch::channel(ProcessingEstimate::default());
    let mut actor = Actor {
        history: History::new(deps.store.clone(), deps.audio_dir.clone()),
        deps,
        hud: hud_tx,
        tx: tx.clone(),
        state: State::Idle,
        epoch: 0,
        recording_started: started_tx,
        processing: processing_tx,
        timings: TimingHistory::default(),
        hide_generation: 0,
        hide_paused: false,
    };
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            actor.handle(m);
        }
    });
    DictationHandle::new(tx, hud_rx, started_rx, processing_rx)
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Wordbook words are a hint only: without them the dictation still runs.
async fn load_wordbook(store: Arc<Store>) -> Vec<String> {
    let loaded = tokio::task::spawn_blocking(move || store.words(None))
        .await
        .map_err(|e| e.to_string())
        .and_then(|words| words.map_err(|e| e.to_string()));
    loaded.unwrap_or_else(|error| {
        tracing::warn!(operation = "load wordbook", %error, "continuing without wordbook words");
        Vec::new()
    })
}

impl Actor {
    fn set_hud(&self, s: HudState) {
        self.hud.send_replace(s);
    }

    fn schedule_hide(&mut self, after: Duration) {
        self.hide_generation += 1;
        self.hide_paused = false;
        let (tx, epoch, generation) = (self.tx.clone(), self.epoch, self.hide_generation);
        tokio::spawn(async move {
            tokio::time::sleep(after).await;
            let _ = tx.send(Msg::Hide(epoch, generation));
        });
    }

    fn go_idle(&mut self) {
        self.hide_paused = false;
        self.epoch += 1;
        self.state = State::Idle;
        self.set_hud(HudState::Hidden);
    }

    fn show_error(
        &mut self,
        kind: ErrorKind,
        detail: String,
        action: ErrorAction,
        retry: Option<(Arc<AudioClip>, Meta)>,
    ) {
        self.state = State::Showing { clip: retry };
        self.set_hud(HudState::Error { kind, detail, action });
        self.schedule_hide(ERROR_LINGER);
    }

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Cmd(c) => self.command(c),
            Msg::PipelinePhase(g, PipelinePhase::Polishing { input_chars, started }) if g == self.epoch => {
                if let State::Processing { models, .. } = self.state {
                    let estimate = self.timings.polishing(models.polish, input_chars);
                    self.processing.send_modify(|p| p.polishing = Some((started, estimate)));
                    self.set_hud(HudState::Polishing);
                }
            }
            Msg::Done(g, r) if g == self.epoch => self.done(r),
            Msg::Delivered(g, delivery) if g == self.epoch => self.delivered(delivery),
            Msg::Hide(g, generation)
                if g == self.epoch
                    && generation == self.hide_generation
                    && !self.hide_paused
                    && matches!(self.state, State::Showing { .. }) =>
            {
                self.go_idle()
            }
            Msg::RecordingLimit(g) if g == self.epoch && matches!(self.state, State::Listening { .. }) => {
                self.finish_listening()
            }
            _ => {}
        }
    }

    fn command(&mut self, c: Command) {
        let mode = self.deps.config.get().general.record_mode;
        match (&self.state, c) {
            (State::Idle | State::Showing { .. }, Command::KeyDown) => self.start_listening(mode),
            (State::Listening { .. }, Command::KeyDown) if mode == RecordMode::Toggle => self.finish_listening(),
            (State::Listening { .. }, Command::KeyUp) if mode == RecordMode::Hold => self.finish_listening(),
            (State::Listening { .. }, Command::Finish) => self.finish_listening(),
            (State::Listening { .. }, Command::Cancel) => self.cancel_listening(),
            (State::Processing { cancel, .. }, Command::Cancel | Command::Stop) => cancel.cancel(),
            (State::Showing { .. }, Command::ErrorHover(over))
                if matches!(*self.hud.borrow(), HudState::Error { .. }) =>
            {
                self.error_hover(over)
            }
            (State::Showing { .. }, Command::Retry) => self.retry(),
            (State::Showing { .. }, Command::Dismiss | Command::Cancel) => self.go_idle(),
            // Insertion cannot be cancelled and a partial one is never retried, so every command but a key press is
            // ignored. One press is kept and starts listening once the outcome is shown.
            (State::Delivering { .. }, Command::KeyDown) => self.state = State::Delivering { key_down: true },
            // Hold mode ends a recording on release, so a press that is already over must not start one.
            (State::Delivering { .. }, Command::KeyUp) if mode == RecordMode::Hold => {
                self.state = State::Delivering { key_down: false }
            }
            // A cancel cannot stop the insertion, but it withdraws a press made during it.
            (State::Delivering { .. }, Command::Cancel) => self.state = State::Delivering { key_down: false },
            _ => {}
        }
    }

    fn error_hover(&mut self, over: bool) {
        if over && !self.hide_paused {
            self.hide_paused = true;
            self.hide_generation += 1;
        } else if !over && self.hide_paused {
            // Leaving always grants a full reading interval, even after a late hover.
            self.schedule_hide(ERROR_LINGER);
        }
    }

    fn start_listening(&mut self, mode: RecordMode) {
        self.epoch += 1;
        if let Err(detail) = self.deps.env.recorder.start() {
            self.show_error(ErrorKind::Mic, detail.to_string(), ErrorAction::OpenSettings, None);
            return;
        }
        self.deps.env.focus_warmer.warm();
        let mute = (self.deps.config.get().general.music_mute == MusicMuteMode::Auto)
            .then(|| self.deps.env.music_mute.mute_if_active());
        let app = self.deps.env.frontmost.frontmost();
        // The app's name is recorded with every entry; the history resolves the label against the current prompts
        // list at display time, so removing an app reads Other and adding it back restores its name.
        let meta = Meta { style: self.deps.config.get().prompts.for_application(&app.bundle_id), app, ts: now() };
        let started = Instant::now();
        self.state = State::Listening { meta, mute };
        self.recording_started.send_replace(Some(started));
        self.set_hud(HudState::Listening { mode });
        self.schedule_recording_limit();
    }

    fn schedule_recording_limit(&self) {
        let (tx, epoch) = (self.tx.clone(), self.epoch);
        let deadline = tokio::time::Instant::now() + MAX_RECORDING_DURATION;
        tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            let _ = tx.send(Msg::RecordingLimit(epoch));
        });
    }

    fn take_listening(&mut self) -> Option<(Meta, Arc<AudioClip>)> {
        let State::Listening { meta, mute } = std::mem::replace(&mut self.state, State::Idle) else {
            return None;
        };
        self.recording_started.send_replace(None);
        let clip = Arc::new(self.deps.env.recorder.stop());
        drop(mute); // music returns only after the microphone has actually stopped
        Some((meta, clip))
    }

    fn cancel_listening(&mut self) {
        let Some((meta, clip)) = self.take_listening() else { return };
        if !clip.is_effectively_silent() {
            // No engine has run yet, so there is nothing to label.
            self.record_history(&meta, &clip, &EngineLabels::default(), HistoryKind::Cancelled, None, None);
        }
        self.go_idle();
    }

    fn finish_listening(&mut self) {
        let Some((meta, clip)) = self.take_listening() else { return };
        self.process(meta, clip);
    }

    fn retry(&mut self) {
        let State::Showing { clip } = std::mem::replace(&mut self.state, State::Idle) else { return };
        match clip {
            Some((clip, meta)) => self.process(meta, clip),
            None => self.go_idle(),
        }
    }

    fn process(&mut self, meta: Meta, clip: Arc<AudioClip>) {
        self.epoch += 1;
        let cancel = CancellationToken::new();
        let epoch = self.epoch;
        let cfg = self.deps.config.get();
        let models = Models::new(&cfg.asr, &cfg.polish.engine);
        let labels = cfg.engine_labels();
        self.processing.send_replace(ProcessingEstimate {
            transcription_secs: self.timings.recognition(models.asr, clip.duration_ms() as f32 / 1000.),
            polishing: None,
        });
        self.set_hud(HudState::Transcribing);
        let (tx, engines, store) = (self.tx.clone(), self.deps.engines.clone(), self.deps.store.clone());
        let (job_clip, style, token) = (clip.clone(), meta.style.clone(), cancel.clone());
        tokio::spawn(async move {
            let words = load_wordbook(store).await;
            let phase_tx = tx.clone();
            let on_phase = move |p: PipelinePhase| {
                if matches!(p, PipelinePhase::Polishing { .. }) {
                    let _ = phase_tx.send(Msg::PipelinePhase(epoch, p));
                }
            };
            let r = pipeline::run(
                engines.as_ref(),
                &cfg,
                &PipelineJob { clip: &job_clip, style, wordbook: &words },
                &on_phase,
                &token,
            )
            .await;
            let _ = tx.send(Msg::Done(epoch, r));
        });
        self.state = State::Processing { meta, clip, cancel, models, labels };
    }

    fn done(&mut self, result: Result<Output, PipelineError>) {
        let State::Processing { meta, clip, models, labels, .. } = std::mem::replace(&mut self.state, State::Idle)
        else {
            return;
        };
        self.epoch += 1;
        match result {
            Ok(out) => {
                self.timings.record(models, clip.duration_ms() as f32 / 1000., &out);
                self.deliver(&meta, &clip, &labels, &out);
            }
            Err(PipelineError::Cancelled) => {
                self.record_history(&meta, &clip, &labels, HistoryKind::Cancelled, None, None);
                self.go_idle();
            }
            Err(PipelineError::Failed(e)) => self.fail(meta, clip, &labels, e),
        }
    }

    /// Saves the dictation, then types it on the blocking pool: the platform sink sleeps while it polls focus and
    /// restores the clipboard, and the actor must keep serving commands meanwhile. History is queued before the
    /// insertion starts, so a crash while typing does not lose the text.
    fn deliver(&mut self, meta: &Meta, clip: &Arc<AudioClip>, labels: &EngineLabels, out: &Output) {
        let polish_error = out.polish_error.as_ref().map(ToString::to_string);
        self.record_history(meta, clip, labels, HistoryKind::Dictation, Some(out), polish_error);
        let method = self.deps.config.get().general.insert_method;
        let (sink, text) = (self.deps.env.sink.clone(), out.text().to_owned());
        let inserting = tokio::task::spawn_blocking(move || sink.deliver(&text, method));
        let (tx, epoch) = (self.tx.clone(), self.epoch);
        tokio::spawn(async move {
            // A panicking sink must not leave the actor waiting for a result that never comes.
            let delivery = inserting.await.unwrap_or_else(|error| {
                tracing::warn!(operation = "deliver text", %error, "insertion task failed");
                Delivery::Failed(error.to_string())
            });
            let _ = tx.send(Msg::Delivered(epoch, delivery));
        });
        self.state = State::Delivering { key_down: false };
    }

    /// The insertion is over: show how it went, then start the recording whose key press was held back meanwhile.
    fn delivered(&mut self, delivery: Delivery) {
        let State::Delivering { key_down } = self.state else { return };
        match delivery {
            Delivery::Inserted => self.show_outcome(HudState::Inserted, SUCCESS_FLASH),
            Delivery::Copied => self.show_outcome(HudState::Copied, COPIED_FLASH),
            Delivery::Failed(detail) => self.show_error(ErrorKind::Other, detail, ErrorAction::Retry, None),
        }
        if key_down {
            let mode = self.deps.config.get().general.record_mode;
            self.start_listening(mode);
        }
    }

    fn show_outcome(&mut self, hud: HudState, linger: Duration) {
        self.state = State::Showing { clip: None };
        self.set_hud(hud);
        self.schedule_hide(linger);
    }

    fn fail(&mut self, meta: Meta, clip: Arc<AudioClip>, labels: &EngineLabels, e: ProviderError) {
        if e.kind != ErrorKind::NoSpeech {
            self.record_history(&meta, &clip, labels, HistoryKind::Failed, None, Some(e.to_string()));
        }
        let action = match e.kind {
            ErrorKind::Mic | ErrorKind::Auth => ErrorAction::OpenSettings,
            _ => ErrorAction::Retry,
        };
        self.show_error(e.kind, e.message, action, Some((clip, meta)));
    }

    fn record_history(
        &self,
        meta: &Meta,
        clip: &Arc<AudioClip>,
        labels: &EngineLabels,
        kind: HistoryKind,
        out: Option<&Output>,
        error: Option<String>,
    ) {
        let entry = NewHistory {
            ts: meta.ts,
            kind,
            style: meta.style.clone(),
            app: meta.app.name.clone(),
            lang: out.map(|o| o.lang.code().to_string()).unwrap_or_default(),
            duration_ms: clip.duration_ms(),
            raw: out.map(|o| o.raw.clone()).unwrap_or_default(),
            polished: out.and_then(|o| o.polished.clone()),
            error,
            audio_path: None,
            engines: labels.clone(),
        };
        self.history.save(clip.clone(), format!("{}-{}.wav", meta.ts, self.epoch), entry);
    }
}

#[cfg(test)]
mod tests;
