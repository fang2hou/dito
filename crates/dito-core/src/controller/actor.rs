use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, Span};

use self::commands::{CommandRun, CommandState, Start};
use super::command::Command;
use super::env::{AppInfo, Delivery, Deps, MuteSession};
use super::handle::{DictationHandle, Msg};
use super::history::History;
use super::hud::{ErrorAction, HudState, RunKind};
use super::progress::{Models, ProcessingEstimate, StepTimes, TimingHistory};
use crate::audio::AudioClip;
use crate::config::{EngineLabels, MusicMuteMode, RecordMode};
use crate::error::{ErrorKind, ProviderError};
use crate::pipeline::{self, Output, PipelineError, PipelineJob, PipelinePhase};
use crate::prompts::AppTarget;
use crate::store::{HistoryKind, NewHistory, StepMillis, Store};
use crate::trace::{self, RunId};
use crate::trace_log;

mod commands;

pub const MAX_RECORDING_DURATION: Duration = Duration::from_secs(5 * 60);

/// Match the HUD's outcome animation; the HUD is hidden once it ends.
const SUCCESS_FLASH: Duration = Duration::from_millis(1430);
const COPIED_FLASH: Duration = Duration::from_millis(1930);
const ERROR_LINGER: Duration = Duration::from_secs(3);
/// `motion/chord-hold`: when the command shortcut is the dictation shortcut plus modifiers, a dictation's capsule
/// waits this long for the extra keys, so a command never flashes in gold first. Recording does not wait.
pub const CHORD_HOLD: Duration = Duration::from_millis(150);

struct Meta {
    app: AppInfo,
    target: AppTarget,
    ts: i64,
}

enum State {
    Idle,
    Listening {
        meta: Meta,
        /// Dropped when the microphone stops, which restores the audio.
        mute: Option<Box<dyn MuteSession>>,
        /// Set when the command shortcut started this recording.
        command: Option<CommandRun>,
        /// When the shortcut that started this recording went down.
        pressed: Instant,
        /// Set while the capsule waits for a handover: the mode it shows once it appears.
        held_capsule: Option<RecordMode>,
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
        /// The dictation shortcut went down meanwhile; in hold mode, releasing it again clears this.
        key_down: bool,
        /// A command's result went over the selection, which reads as `Replaced` instead of `Inserted`.
        replaced: bool,
        /// The command whose result is being delivered; its history row is written with the outcome.
        run: Option<CommandRun>,
    },
    /// A command run past its recording: a step is running or a window waits for the user.
    Command(CommandState),
    Showing {
        clip: Option<(Arc<AudioClip>, Meta)>,
    },
}

impl State {
    fn trace_name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Listening { command: None, .. } => "listening",
            Self::Listening { command: Some(_), .. } => "listening for a command",
            Self::Processing { .. } => "processing",
            Self::Delivering { .. } => "delivering",
            Self::Command(state) => state.trace_name(),
            Self::Showing { .. } => "showing",
        }
    }
}

struct Actor {
    deps: Deps,
    history: History,
    hud: watch::Sender<HudState>,
    run_kind: watch::Sender<RunKind>,
    tx: mpsc::UnboundedSender<Msg>,
    state: State,
    /// Each timer and task carries the value it started under and is ignored once this has moved on. Not every
    /// transition bumps it, so a handler checks the state as well.
    epoch: u64,
    recording_started: watch::Sender<Option<Instant>>,
    processing: watch::Sender<ProcessingEstimate>,
    timings: TimingHistory,
    step_times: watch::Sender<StepTimes>,
    hide_generation: u64,
    hide_paused: bool,
    /// The debug-log span of the latest run; every message is handled inside it, and the tasks of the run carry it.
    span: Span,
}

pub fn spawn(deps: Deps) -> DictationHandle {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (hud_tx, hud_rx) = watch::channel(HudState::Hidden);
    let (run_kind_tx, run_kind_rx) = watch::channel(RunKind::default());
    let (started_tx, started_rx) = watch::channel(None);
    let (processing_tx, processing_rx) = watch::channel(ProcessingEstimate::default());
    let (step_times_tx, step_times_rx) = watch::channel(StepTimes::default());
    let mut actor = Actor {
        history: History::new(deps.store.clone(), deps.audio_dir.clone()),
        deps,
        hud: hud_tx,
        run_kind: run_kind_tx,
        tx: tx.clone(),
        state: State::Idle,
        epoch: 0,
        recording_started: started_tx,
        processing: processing_tx,
        timings: TimingHistory::default(),
        step_times: step_times_tx,
        hide_generation: 0,
        hide_paused: false,
        span: Span::none(),
    };
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            actor.handle(m);
        }
    });
    DictationHandle::new(tx, hud_rx, run_kind_rx, started_rx, processing_rx, step_times_rx)
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
        trace_log!("run ended, the HUD is hidden");
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
        trace_log!(kind = ?kind, detail = %detail, action = ?action, "error shown");
        self.state = State::Showing { clip: retry };
        self.set_hud(HudState::Error { kind, detail, action });
        self.schedule_hide(ERROR_LINGER);
    }

    /// Like an error for the shortcuts, the linger and the hover, but nothing failed and there is nothing to retry.
    fn show_not_recognized(&mut self) {
        trace_log!("instruction not recognized, notice shown");
        self.state = State::Showing { clip: None };
        self.set_hud(HudState::NotRecognized);
        self.schedule_hide(ERROR_LINGER);
    }

    fn handle(&mut self, m: Msg) {
        let span = self.span.clone();
        let _entered = span.enter();
        match m {
            Msg::Cmd(c) => self.command(c),
            Msg::PipelinePhase(g, PipelinePhase::Polishing { input_chars, started }) if g == self.epoch => {
                if let State::Processing { models, .. } = self.state {
                    let estimate = self.timings.polishing(models.polish, input_chars);
                    self.processing.send_modify(|p| p.polishing = Some((started, estimate)));
                    self.set_hud(HudState::Polishing);
                }
            }
            Msg::Done(g, r) if g == self.epoch && matches!(self.state, State::Processing { .. }) => self.done(r),
            Msg::Step(g, r) if g == self.epoch => self.step_result(r),
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
            Msg::RevealCapsule(g) if g == self.epoch => self.reveal_capsule(),
            _ => {}
        }
    }

    fn command(&mut self, c: Command) {
        let mode = self.deps.config.get().general.record_mode;
        // A shortcut that starts a run is logged as that run's first line, inside the run's own span.
        let starts_run = matches!(self.state, State::Idle | State::Showing { .. })
            && matches!(c, Command::KeyDown | Command::KeyDownMayHandOver | Command::CommandKeyDown);
        // The same press, except that a possible handover holds the capsule back.
        let (c, may_hand_over) = match c {
            Command::KeyDownMayHandOver => (Command::KeyDown, true),
            c => (c, false),
        };
        if !starts_run && !matches!(c, Command::ErrorHover(_)) {
            trace_log!(command = ?c, state = self.state.trace_name(), "command received");
        }
        match (&self.state, c) {
            (State::Idle | State::Showing { .. }, Command::KeyDown) => {
                self.start_recording(mode, Start::Dictation, may_hand_over)
            }
            (State::Idle | State::Showing { .. }, Command::CommandKeyDown) => self.start_command(mode),
            // Toggle: a press of either shortcut ends the recording, whichever kind of run it is, and that press
            // does nothing else: the handover and chord messages that follow it from the same keys find no
            // recording and are ignored.
            (State::Listening { .. }, Command::KeyDown | Command::CommandKeyDown) if mode == RecordMode::Toggle => {
                self.finish_listening()
            }
            // Hold: releasing the shortcut that started the recording ends it; the other shortcut is ignored.
            (State::Listening { command: None, .. }, Command::KeyUp) if mode == RecordMode::Hold => {
                self.finish_listening()
            }
            (State::Listening { command: Some(_), .. }, Command::CommandKeyUp) if mode == RecordMode::Hold => {
                self.finish_listening()
            }
            // The press that started this recording turned out to be the other shortcut's: same run, other kind.
            (State::Listening { command: None, .. }, Command::KeyToCommand) => {
                self.trace_handover();
                self.convert_to_command();
                self.reveal_capsule();
            }
            (State::Listening { command: Some(_), .. }, Command::CommandToKey) => {
                self.trace_handover();
                self.convert_to_dictation();
            }
            // Typing, right after the press that started this recording.
            (State::Listening { .. }, Command::ChordAbort) => self.cancel_listening(),
            (State::Listening { .. }, Command::Finish) => self.finish_listening(),
            (State::Listening { .. }, Command::Cancel) => self.cancel_listening(),
            (State::Listening { command: Some(_), .. }, Command::PolishSelection) => self.polish_selection(),
            (State::Listening { command: Some(_), .. }, Command::TypeInstruction) => self.type_instruction(),
            (State::Processing { cancel, .. }, Command::Cancel | Command::Stop) => cancel.cancel(),
            (State::Command(_), Command::Cancel | Command::Stop | Command::Dismiss) => self.close_command(),
            (State::Command(_), Command::SubmitInstruction(text)) => self.submit_instruction(text),
            (State::Command(_), Command::SubmitText(text)) => self.submit_text(text),
            (State::Command(_), Command::Choose(intent)) => self.choose(intent),
            (State::Command(_), Command::Replace(text)) => self.replace_result(text),
            (State::Command(_), Command::CopyResult(text)) => self.copy_result(text),
            (State::Showing { .. }, Command::ErrorHover(over))
                if matches!(*self.hud.borrow(), HudState::Error { .. } | HudState::NotRecognized) =>
            {
                self.error_hover(over)
            }
            (State::Showing { .. }, Command::Retry) => self.retry(),
            (State::Showing { .. }, Command::Dismiss | Command::Cancel) => self.go_idle(),
            // Insertion cannot be cancelled and a partial one is never retried, so every command but a key press is
            // ignored. One press is kept and starts listening once the outcome is shown.
            (State::Delivering { .. }, Command::KeyDown) => self.hold_pending_press(true),
            // Hold mode ends a recording on release, so a press that is already over must not start one.
            (State::Delivering { .. }, Command::KeyUp) if mode == RecordMode::Hold => self.hold_pending_press(false),
            // A cancel cannot stop the insertion, but it withdraws a press made during it.
            (State::Delivering { .. }, Command::Cancel | Command::ChordAbort) => self.hold_pending_press(false),
            _ => {}
        }
    }

    fn hold_pending_press(&mut self, down: bool) {
        if let State::Delivering { key_down, .. } = &mut self.state {
            *key_down = down;
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

    fn start_listening(&mut self, mode: RecordMode, start: Start) {
        self.start_recording(mode, start, false)
    }

    /// `hold_capsule`: the capsule waits [`CHORD_HOLD`] before it shows the recording, unless a handover or the next
    /// press decides it sooner. The recording, its timers and the run's state start now either way.
    fn start_recording(&mut self, mode: RecordMode, start: Start, hold_capsule: bool) {
        self.epoch += 1;
        let kind = match start {
            Start::Dictation => RunKind::Dictation,
            Start::Command { .. } => RunKind::Command,
        };
        self.run_kind.send_replace(kind);
        let id = RunId::new(if kind == RunKind::Command { 'c' } else { 'd' });
        self.span = trace::run_span(&id);
        let span = self.span.clone();
        let _entered = span.enter();
        trace_log!(kind = ?kind, mode = ?mode, hold_capsule, "shortcut down, run started");
        if let Start::Command { selection } = &start {
            commands::trace_selection(selection.as_deref(), None);
        }
        if let Err(detail) = self.deps.env.recorder.start() {
            trace_log!(error = %detail, "recording did not start");
            self.show_error(ErrorKind::Mic, detail.to_string(), ErrorAction::OpenSettings, None);
            return;
        }
        trace_log!("recording started");
        self.deps.env.focus_warmer.warm();
        let mute = (self.deps.config.get().general.music_mute == MusicMuteMode::Auto)
            .then(|| self.deps.env.music_mute.mute_if_active());
        let app = self.deps.env.frontmost.frontmost();
        // The app's name is recorded with every entry; the history resolves the label against the current prompts
        // list at display time, so removing an app reads Other and adding it back restores its name.
        let meta = Meta { target: self.deps.config.get().prompts.for_application(&app.bundle_id), app, ts: now() };
        let command = match start {
            Start::Dictation => None,
            Start::Command { selection } => Some(CommandRun::new(selection, &meta)),
        };
        let started = Instant::now();
        let held_capsule = hold_capsule.then_some(mode);
        self.state = State::Listening { meta, mute, command, pressed: started, held_capsule };
        self.recording_started.send_replace(Some(started));
        if hold_capsule {
            // Whatever the HUD showed (an outcome, an error) gives way now, as it would to the capsule.
            self.set_hud(HudState::Hidden);
            self.schedule_capsule_reveal();
        } else {
            self.set_hud(HudState::Listening { mode });
        }
        self.schedule_recording_limit();
    }

    fn schedule_capsule_reveal(&self) {
        let (tx, epoch) = (self.tx.clone(), self.epoch);
        tokio::spawn(async move {
            tokio::time::sleep(CHORD_HOLD).await;
            let _ = tx.send(Msg::RevealCapsule(epoch));
        });
    }

    /// Shows the capsule of a recording that held it back; a recording whose capsule is already up is untouched.
    fn reveal_capsule(&mut self) {
        let State::Listening { held_capsule, .. } = &mut self.state else { return };
        if let Some(mode) = held_capsule.take() {
            self.set_hud(HudState::Listening { mode });
        }
    }

    /// How long after the first key the other shortcut's keys arrived, for tuning [`CHORD_HOLD`].
    fn trace_handover(&self) {
        if let State::Listening { pressed, held_capsule, .. } = &self.state {
            trace_log!(
                chord_gap_ms = pressed.elapsed().as_millis() as u64,
                capsule_held = held_capsule.is_some(),
                "the press was handed over to the other shortcut"
            );
        }
    }

    fn schedule_recording_limit(&self) {
        let (tx, epoch) = (self.tx.clone(), self.epoch);
        let deadline = tokio::time::Instant::now() + MAX_RECORDING_DURATION;
        tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            let _ = tx.send(Msg::RecordingLimit(epoch));
        });
    }

    fn take_listening(&mut self) -> Option<(Meta, Arc<AudioClip>, Option<CommandRun>)> {
        let State::Listening { meta, mute, command, .. } = std::mem::replace(&mut self.state, State::Idle) else {
            return None;
        };
        self.recording_started.send_replace(None);
        let clip = Arc::new(self.deps.env.recorder.stop());
        drop(mute); // music returns only after the microphone has actually stopped
        trace_log!(
            duration_ms = clip.duration_ms(),
            rms = %format!("{:.4}", clip.rms()),
            peak = %format!("{:.4}", clip.peak()),
            silent = clip.is_effectively_silent(),
            "recording stopped"
        );
        Some((meta, clip, command))
    }

    fn cancel_listening(&mut self) {
        let Some((meta, clip, command)) = self.take_listening() else { return };
        trace_log!("recording cancelled");
        // A command cancelled while recording leaves nothing behind: nothing was decided yet and its audio is not kept.
        if command.is_none() && !clip.is_effectively_silent() {
            // No engine has run yet, so there is nothing to label.
            self.record_history(&meta, &clip, &EngineLabels::default(), HistoryKind::Cancelled, None, None);
        }
        self.go_idle();
    }

    fn finish_listening(&mut self) {
        let Some((meta, clip, command)) = self.take_listening() else { return };
        match command {
            Some(run) => self.transcribe_instruction(clip, run),
            None => self.process(meta, clip),
        }
    }

    fn retry(&mut self) {
        let State::Showing { clip } = std::mem::replace(&mut self.state, State::Idle) else { return };
        match clip {
            Some((clip, meta)) => {
                trace_log!("retrying the dictation with the same recording");
                self.process(meta, clip)
            }
            None => self.go_idle(),
        }
    }

    fn process(&mut self, meta: Meta, clip: Arc<AudioClip>) {
        self.epoch += 1;
        let cancel = CancellationToken::new();
        let epoch = self.epoch;
        let cfg = self.deps.config.get();
        let models = Models::new(&cfg.asr, &cfg.text_model);
        let labels = cfg.engine_labels();
        self.processing.send_replace(ProcessingEstimate {
            transcription_secs: self.timings.transcription(models.asr, clip.duration_ms() as f32 / 1000.),
            polishing: None,
        });
        self.set_hud(HudState::Transcribing);
        trace_log!(
            audio_ms = clip.duration_ms(),
            app = %meta.app.name,
            style = ?meta.target,
            asr = ?labels.asr,
            polish = ?labels.polish,
            "dictation processing started"
        );
        let (tx, engines, store) = (self.tx.clone(), self.deps.engines.clone(), self.deps.store.clone());
        let (job_clip, style, token) = (clip.clone(), meta.target.clone(), cancel.clone());
        let span = self.span.clone();
        tokio::spawn(
            async move {
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
                    &PipelineJob { clip: &job_clip, target: style, wordbook: &words },
                    &on_phase,
                    &token,
                )
                .await;
                let _ = tx.send(Msg::Done(epoch, r));
            }
            .instrument(span),
        );
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
                trace_log!(
                    asr_ms = out.asr_ms,
                    polish_ms = out.polish_ms,
                    polished = out.polished.is_some(),
                    polish_error = ?out.polish_error.as_ref().map(ToString::to_string),
                    text = ?out.text(),
                    "dictation processed"
                );
                self.timings.record(models, clip.duration_ms() as f32 / 1000., &out);
                self.step_times.send_modify(|times| times.record(models, &out));
                self.deliver(&meta, &clip, &labels, &out);
            }
            Err(PipelineError::Cancelled) => {
                trace_log!("dictation cancelled while processing");
                self.record_history(&meta, &clip, &labels, HistoryKind::Cancelled, None, None);
                self.go_idle();
            }
            Err(PipelineError::Failed(e)) => {
                trace_log!(kind = ?e.kind, error = %e, "dictation failed");
                self.fail(meta, clip, &labels, e)
            }
        }
    }

    /// Typing runs on the blocking pool: the platform sink sleeps while it polls focus and restores the clipboard, and
    /// the actor must keep serving commands meanwhile. History is queued before the insertion starts, so a crash while
    /// typing does not lose the text.
    fn deliver(&mut self, meta: &Meta, clip: &Arc<AudioClip>, labels: &EngineLabels, out: &Output) {
        let polish_error = out.polish_error.as_ref().map(ToString::to_string);
        self.record_history(meta, clip, labels, HistoryKind::Dictation, Some(out), polish_error);
        let method = self.deps.config.get().general.insert_method;
        let (sink, text) = (self.deps.env.sink.clone(), out.text().to_owned());
        trace_log!(?method, chars = text.chars().count(), "delivering at the cursor");
        self.start_delivery(move || sink.deliver(&text, method), false, None);
    }

    /// Runs `work` on the blocking pool and enters `Delivering`; its result comes back as `Msg::Delivered`. A command
    /// passes its `run`, whose history row is written once the outcome is known.
    fn start_delivery(
        &mut self,
        work: impl FnOnce() -> Delivery + Send + 'static,
        replaced: bool,
        run: Option<CommandRun>,
    ) {
        let span = self.span.clone();
        let inserting = tokio::task::spawn_blocking({
            let span = span.clone();
            move || span.in_scope(work)
        });
        let (tx, epoch) = (self.tx.clone(), self.epoch);
        tokio::spawn(
            async move {
                // A panicking sink must not leave the actor waiting for a result that never comes.
                let delivery = inserting.await.unwrap_or_else(|error| {
                    tracing::warn!(operation = "deliver text", %error, "insertion task failed");
                    Delivery::Failed(error.to_string())
                });
                let _ = tx.send(Msg::Delivered(epoch, delivery));
            }
            .instrument(span),
        );
        self.state = State::Delivering { key_down: false, replaced, run };
    }

    /// Starts the recording whose key press was held back during the insertion.
    fn delivered(&mut self, delivery: Delivery) {
        let State::Delivering { key_down, replaced, run } = &mut self.state else { return };
        let (key_down, replaced, run) = (*key_down, *replaced, run.take());
        trace_log!(delivery = ?delivery, "delivery finished");
        if let Some(run) = run {
            self.finish_delivered_command(run, &delivery, replaced);
        }
        match delivery {
            Delivery::Inserted if replaced => self.show_outcome(HudState::Replaced, SUCCESS_FLASH),
            Delivery::Inserted => self.show_outcome(HudState::Inserted, SUCCESS_FLASH),
            Delivery::Copied => self.show_outcome(HudState::Copied, COPIED_FLASH),
            Delivery::Failed(detail) => self.show_error(ErrorKind::Other, detail, ErrorAction::Retry, None),
        }
        if key_down {
            let mode = self.deps.config.get().general.record_mode;
            self.start_listening(mode, Start::Dictation);
        }
    }

    fn show_outcome(&mut self, hud: HudState, linger: Duration) {
        trace_log!(outcome = ?hud, "outcome shown");
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
            style: meta.target.clone(),
            app: meta.app.name.clone(),
            lang: out.map(|o| o.lang.code().to_string()).unwrap_or_default(),
            duration_ms: clip.duration_ms(),
            raw: out.map(|o| o.raw.clone()).unwrap_or_default(),
            polished: out.and_then(|o| o.polished.clone()),
            error,
            audio_path: None,
            engines: labels.clone(),
            timings: StepMillis {
                asr: out.map(|o| o.asr_ms),
                polish: out.filter(|o| o.polished.is_some()).map(|o| o.polish_ms),
            },
            source: None,
            command: None,
        };
        trace_log!(kind = ?kind, "history row queued");
        self.history.save(clip.clone(), format!("{}-{}.wav", meta.ts, self.epoch), entry);
    }
}

#[cfg(test)]
mod tests;
