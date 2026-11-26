//! The command flow of the actor: selection, instruction, routing, tool, windows and delivery. One run is a chain of
//! steps on the Tokio runtime, each answered by one `Msg::Step`, and windows where the user has to decide something.
//! A run that gets past its recording ends in one history row of the kind `Command`, whatever the outcome.

use std::sync::Arc;
use std::time::Instant;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::super::env::{CapturedSelection, Delivery};
use super::super::handle::{Msg, StepResult};
use super::super::hud::{Choice, ErrorAction, HudState, ResultWrite, RunKind};
use super::super::progress::{Models, ProcessingEstimate};
use super::{Actor, Meta, State, load_wordbook};
use crate::audio::AudioClip;
use crate::command::{Heard, Tool, ToolContext, needs_text};
use crate::config::{Config, EngineLabels, RecordMode};
use crate::error::{ErrorKind, ProviderError};
use crate::factory::Engines;
use crate::intent::{AUTO_THRESHOLD, Decision, Intent};
use crate::pipeline::{self, PipelineError, or_cancel};
use crate::prompts::{AppTarget, detect_lang};
use crate::store::{CommandDetails, CommandOutcome, HistoryKind, InstructionOrigin, NewHistory, SourceKind};
use crate::store::{StepMillis, Store, ToolChoice, wordbook_terms};
use crate::surrounding::{Surrounding, context_tokens};
use crate::trace;
use crate::trace_log;

pub(super) enum Start {
    Dictation,
    Command,
}

pub(super) struct CommandRun {
    /// Captured at shortcut down and refreshed when recording ends, unless the second read fails.
    selection: Option<Arc<dyn CapturedSelection>>,
    selection_capture_pending: bool,
    instruction_ready: bool,
    /// Frontmost app target captured at shortcut down.
    target: AppTarget,
    /// Empty when started from the polish button.
    instruction: String,
    record: Record,
}

/// What the history row of the run is built from, filled in as the run goes.
#[derive(Default)]
struct Record {
    ts: i64,
    app: String,
    /// How long the instruction was recorded; zero when it was typed or skipped.
    recording_ms: u64,
    lang: Option<String>,
    origin: Option<InstructionOrigin>,
    asr_engine: Option<String>,
    polish_engine: Option<String>,
    router_engine: Option<String>,
    asr_ms: Option<u64>,
    route_ms: Option<u64>,
    tool_ms: Option<u64>,
    decision: Option<Decision>,
    choice: Option<ToolChoice>,
    tool: Option<Intent>,
    source: Option<(SourceKind, String)>,
    /// The text that was delivered, or the answer; for a window it is the text the window showed.
    result: Option<String>,
}

impl CommandRun {
    pub(super) fn new(selection: Option<Arc<dyn CapturedSelection>>, meta: &Meta) -> Self {
        let record = Record { ts: meta.ts, app: meta.app.name.clone(), ..Record::default() };
        Self {
            selection,
            selection_capture_pending: false,
            instruction_ready: false,
            target: meta.target.clone(),
            instruction: String::new(),
            record,
        }
    }

    /// The captured selection is also there when only the field's text was read; then nothing is selected.
    fn selected_text(&self) -> Option<&str> {
        self.selection.as_ref().map(|selection| selection.text()).filter(|text| !text.is_empty())
    }

    /// Whether the selected text can be written over. `false` without a selection.
    fn selection_editable(&self) -> bool {
        self.selection.as_ref().is_some_and(|selection| selection.editable())
    }

    /// The text around the selection, cut to what a tool run on `text` may carry.
    fn fitted_surrounding(&self, text: Option<&str>) -> Option<Surrounding> {
        let context = self.selection.as_ref()?.surrounding()?;
        let fitted = context.fit(text.unwrap_or_default());
        trace_log!(
            context_chars = context.chars(),
            fitted_chars = fitted.as_ref().map_or(0, Surrounding::chars),
            budget_tokens = context_tokens(text.unwrap_or_default()),
            "surrounding text fitted"
        );
        fitted
    }
}

pub(super) struct CommandState {
    run: CommandRun,
    phase: Phase,
}

impl CommandState {
    pub(super) fn trace_name(&self) -> &'static str {
        match self.phase {
            Phase::Working { .. } => "working",
            Phase::Choosing => "choosing",
            Phase::TypingInstruction => "typing the instruction",
            Phase::NeedsText { .. } => "needing text",
            Phase::Answer => "showing the answer",
            Phase::Confirm { .. } => "confirming",
        }
    }
}

enum Phase {
    /// A step is running. Windows wait for a command and never hide on their own.
    Working {
        cancel: CancellationToken,
    },
    Choosing,
    TypingInstruction,
    NeedsText {
        intent: Intent,
    },
    Answer,
    Confirm {
        write: ResultWrite,
    },
}

/// What a step needs from the actor, owned so the step can outlive it.
struct StepEnv {
    engines: Arc<dyn Engines>,
    cfg: Arc<Config>,
    store: Arc<Store>,
    cancel: CancellationToken,
}

fn cancelled() -> ProviderError {
    ProviderError::new(ErrorKind::Other, "cancelled")
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

async fn transcribe_step(env: StepEnv, clip: Arc<AudioClip>) -> StepResult {
    let entries = load_wordbook(env.store).await;
    let transcript = pipeline::transcribe_clip(env.engines.as_ref(), &env.cfg, &clip, &entries, &env.cancel).await;
    StepResult::Instruction(transcript)
}

async fn route_step(env: StepEnv, instruction: String) -> StepResult {
    trace_log!(instruction = ?instruction, "router request");
    let started = Instant::now();
    let decide = async {
        let router = env.engines.router(&env.cfg).await?;
        router.decide(&instruction).await
    };
    let result = or_cancel(&env.cancel, decide).await.unwrap_or_else(|_| Err(cancelled()));
    let ms = elapsed_ms(started);
    match &result {
        Ok(d) => trace_log!(
            route_ms = ms,
            intent = d.intent.label(),
            confidence = %format!("{:.3}", d.confidence),
            gate = %format!("{AUTO_THRESHOLD:.2}"),
            gate_passed = d.is_confident(),
            probabilities = %crate::trace::ranked_line(d),
            rewrite_op = ?d.rewrite_op.map(|(op, p)| format!("{}={p:.3}", op.label())),
            "router decision"
        ),
        Err(error) => trace_log!(route_ms = ms, %error, "router failed"),
    }
    StepResult::Routed { result, ms }
}

async fn tool_step(
    env: StepEnv,
    intent: Intent,
    write: ResultWrite,
    tool: Tool,
    style: AppTarget,
    surrounding: Option<Surrounding>,
) -> StepResult {
    trace_log!(tool = tool.name(), "tool started");
    let started = Instant::now();
    let run = async {
        let text_model = env.engines.configured_text_model(&env.cfg).await?;
        let words = if matches!(tool, Tool::Polish { .. }) {
            wordbook_terms(&load_wordbook(env.store.clone()).await)
        } else {
            Vec::new()
        };
        let ctx = ToolContext {
            prompts: &env.cfg.prompts,
            target: style,
            wordbook: pipeline::polish_wordbook(&env.cfg, &words),
            surrounding: surrounding.as_ref(),
        };
        let text = tool.run(text_model.as_ref(), ctx).await?;
        // An empty result would erase the selection it replaces.
        if text.trim().is_empty() {
            return Err(ProviderError::new(ErrorKind::BadResponse, "empty model output"));
        }
        Ok(text)
    };
    let result = or_cancel(&env.cancel, run).await.unwrap_or_else(|_| Err(cancelled()));
    let ms = elapsed_ms(started);
    trace_log!(tool = tool.name(), tool_ms = ms, ok = result.is_ok(), "tool finished");
    StepResult::Tool { intent, write, result, ms }
}

/// `replaced`: set for the reading at the end of the recording, whether it replaced the first one.
pub(super) fn trace_selection(selection: Option<&dyn CapturedSelection>, replaced: Option<bool>) {
    match selection {
        Some(selection) if !selection.text().is_empty() => trace_log!(
            chars = selection.text().chars().count(),
            preview = %trace::preview(selection.text()),
            context_chars = selection.surrounding().map(Surrounding::chars),
            editable = selection.editable(),
            replaced,
            "selection captured"
        ),
        Some(selection) => trace_log!(
            context_chars = selection.surrounding().map(Surrounding::chars),
            replaced,
            "nothing selected, the field's text was read"
        ),
        None => trace_log!(replaced, "nothing selected"),
    }
}

impl Actor {
    /// The microphone starts before the selection is read, so its Copy delays cannot clip the user's first words.
    pub(super) fn start_command(&mut self, mode: RecordMode) {
        self.start_listening(mode, Start::Command);
    }

    /// The command shortcut completed the press that started this dictation: the recording, its run and its HUD go on
    /// as a command, and the selection is read now, a moment after the press.
    pub(super) fn convert_to_command(&mut self) {
        let selection = self.deps.env.selection.capture();
        let State::Listening { meta, command, .. } = &mut self.state else { return };
        trace_log!("the dictation became a command");
        trace_selection(selection.as_deref(), None);
        *command = Some(CommandRun::new(selection, meta));
        self.run_kind.send_replace(RunKind::Command);
    }

    /// The reverse of [`Self::convert_to_command`]; the selection is dropped unread.
    pub(super) fn convert_to_dictation(&mut self) {
        let State::Listening { command, .. } = &mut self.state else { return };
        trace_log!("the command became a dictation");
        *command = None;
        self.run_kind.send_replace(RunKind::Dictation);
    }

    /// Reads the selection again when the recording of a command ends, so the user can change it while speaking. The
    /// second reading replaces the first, even when it finds nothing selected: the user deselected. It is dropped when
    /// it fails (nothing readable, or an error), because then the first is all there is.
    fn reread_selection(&self, run: &mut CommandRun) {
        self.apply_reread_selection(run, self.deps.env.selection.capture());
    }

    fn apply_reread_selection(&self, run: &mut CommandRun, selection: Option<Arc<dyn CapturedSelection>>) {
        match selection {
            Some(selection) => {
                trace_selection(Some(selection.as_ref()), Some(true));
                run.selection = Some(selection);
            }
            None => trace_log!(
                replaced = false,
                kept_chars = run.selection.as_ref().map(|selection| selection.text().chars().count()),
                "nothing was read again, the first selection is kept"
            ),
        }
    }

    /// Stops the recording of a command run and drops the audio: unlike a dictation, it is not kept. The selection is
    /// read again, because the run goes on with the buttons' own paths.
    fn abandon_recording(&mut self) -> Option<CommandRun> {
        let mut run = self.take_listening().and_then(|(_, _, run)| run)?;
        self.reread_selection(&mut run);
        Some(run)
    }

    pub(super) fn polish_selection(&mut self) {
        if let Some(run) = self.abandon_recording() {
            trace_log!("polish button pressed, the recording is dropped and routing is skipped");
            self.run_intent(run, Intent::Polish, ToolChoice::PolishButton);
        }
    }

    pub(super) fn type_instruction(&mut self) {
        if let Some(run) = self.abandon_recording() {
            trace_log!("keyboard button pressed, the recording is dropped");
            self.show_window(run, Phase::TypingInstruction, HudState::TypingInstruction);
        }
    }

    pub(super) fn transcribe_instruction(&mut self, clip: Arc<AudioClip>, mut run: CommandRun) {
        let reread_selection = !clip.is_effectively_silent();
        run.selection_capture_pending = reread_selection;
        let cfg = self.deps.config.get();
        let models = Models::new(&cfg.asr, &cfg.text_model);
        self.processing.send_replace(ProcessingEstimate {
            transcription_secs: self.timings.transcription(models.asr, clip.duration_ms() as f32 / 1000.),
            polishing: None,
        });
        run.record.origin = Some(InstructionOrigin::Spoken);
        run.record.recording_ms = clip.duration_ms();
        run.record.asr_engine = Some(cfg.asr.label());
        trace_log!(audio_ms = clip.duration_ms(), engine = %cfg.asr.label(), "instruction sent to speech recognition");
        self.start_step(run, HudState::Transcribing, move |env| transcribe_step(env, clip));
        if reread_selection {
            self.capture_selection_in_background();
        }
    }

    fn capture_selection_in_background(&self) {
        let (selection, tx, epoch, span) =
            (self.deps.env.selection.clone(), self.tx.clone(), self.epoch, self.span.clone());
        tokio::task::spawn_blocking(move || {
            let captured =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| span.in_scope(|| selection.capture())));
            let captured = captured.unwrap_or_else(|_| {
                tracing::warn!(operation = "capture selection", "selection task failed");
                None
            });
            let _ = tx.send(Msg::SelectionCaptured(epoch, captured));
        });
    }

    pub(super) fn selection_captured(&mut self, selection: Option<Arc<dyn CapturedSelection>>) {
        let Some((mut run, phase)) = self.take_command(|phase| matches!(phase, Phase::Working { .. })) else {
            return;
        };
        if !run.selection_capture_pending {
            self.state = State::Command(CommandState { run, phase });
            return;
        }
        self.apply_reread_selection(&mut run, selection);
        run.selection_capture_pending = false;
        if run.instruction_ready {
            self.route(run);
        } else {
            self.state = State::Command(CommandState { run, phase });
        }
    }

    fn route(&mut self, mut run: CommandRun) {
        run.instruction_ready = false;
        let instruction = run.instruction.clone();
        // The router is always the local chat model, whatever the polish source is.
        run.record.router_engine = Some(format!("local:{}", self.deps.config.get().text_model.local_model));
        self.start_step(run, HudState::Thinking, move |env| route_step(env, instruction));
    }

    /// Cancel, Stop and Dismiss: a running step is dropped (which stops local inference) and a window just closes.
    pub(super) fn close_command(&mut self) {
        let State::Command(state) = &self.state else { return };
        if let Phase::Working { cancel } = &state.phase {
            cancel.cancel();
        }
        let State::Command(CommandState { run, phase }) = std::mem::replace(&mut self.state, State::Idle) else {
            unreachable!("the state was a command a moment ago");
        };
        let outcome = match phase {
            Phase::Working { .. } => CommandOutcome::Cancelled,
            Phase::Answer => CommandOutcome::Answered,
            _ => CommandOutcome::Closed,
        };
        trace_log!(outcome = ?outcome, "the user closed the command");
        self.finish_run(run, outcome, None);
        self.go_idle();
    }

    pub(super) fn submit_instruction(&mut self, text: String) {
        let instruction = text.trim();
        if instruction.is_empty() {
            return;
        }
        let instruction = instruction.to_owned();
        if let Some((mut run, _)) = self.take_command(|p| matches!(p, Phase::TypingInstruction)) {
            trace_log!(instruction = ?instruction, "the user typed an instruction");
            run.instruction = instruction;
            run.record.origin = Some(InstructionOrigin::Typed);
            self.route(run);
        }
    }

    pub(super) fn submit_text(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        if let Some((run, Phase::NeedsText { intent })) = self.take_command(|p| matches!(p, Phase::NeedsText { .. })) {
            trace_log!(text = ?text, "the user entered the text the tool asked for");
            self.run_tool(run, intent, Some(text), false);
        }
    }

    pub(super) fn choose(&mut self, intent: Intent) {
        if let Some((run, _)) = self.take_command(|p| matches!(p, Phase::Choosing)) {
            trace_log!(intent = intent.label(), "the user chose an intent");
            self.run_intent(run, intent, ToolChoice::UserChoice);
        }
    }

    pub(super) fn replace_result(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        if let Some((run, Phase::Confirm { write })) =
            self.take_command(|p| matches!(p, Phase::Confirm { write } if *write != ResultWrite::CopyOnly))
        {
            trace_log!(text = ?text, write = ?write, "the user confirmed the result");
            self.deliver_command(run, text, write == ResultWrite::ReplaceSelection);
        }
    }

    /// The window stays until the outcome replaces it.
    pub(super) fn copy_result(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        if let Some((mut run, _)) = self.take_command(|p| matches!(p, Phase::Confirm { .. } | Phase::Answer)) {
            trace_log!(text = ?text, "the user copied the result");
            self.epoch += 1;
            run.record.result = Some(text.clone());
            let sink = self.deps.env.sink.clone();
            self.start_delivery(move || sink.copy(&text), false, Some(run));
        }
    }

    pub(super) fn step_result(&mut self, result: StepResult) {
        let Some((mut run, phase)) = self.take_command(|p| matches!(p, Phase::Working { .. })) else { return };
        match result {
            StepResult::Instruction(Ok(out)) => {
                trace_log!(instruction = ?out.raw, asr_ms = out.asr_ms, "the instruction was transcribed");
                run.record.asr_ms = Some(out.asr_ms);
                run.record.lang = Some(out.lang.code().to_string());
                run.instruction = out.raw;
                run.instruction_ready = true;
                if !run.selection_capture_pending {
                    self.route(run);
                } else {
                    self.state = State::Command(CommandState { run, phase });
                }
            }
            StepResult::Instruction(Err(PipelineError::Cancelled)) => {
                self.finish_run(run, CommandOutcome::Cancelled, None);
                self.go_idle();
            }
            StepResult::Instruction(Err(PipelineError::Failed(e))) => self.command_error(run, e),
            StepResult::Routed { result: Err(e), ms } => {
                run.record.route_ms = Some(ms);
                self.command_error(run, e);
            }
            StepResult::Routed { result: Ok(decision), ms } => {
                run.record.route_ms = Some(ms);
                self.decided(run, decision);
            }
            StepResult::Tool { result: Err(e), ms, .. } => {
                run.record.tool_ms = Some(ms);
                self.command_error(run, e);
            }
            StepResult::Tool { intent, write, result: Ok(text), ms } => {
                run.record.tool_ms = Some(ms);
                self.finish_tool(run, intent, write, text)
            }
        }
    }

    /// Below the confidence gate the user chooses; cancel is never offered, only taken when sure. A likeliest intent
    /// of none shows the not-recognized notice, sure or not, because the window has nothing to offer for it.
    fn decided(&mut self, mut run: CommandRun, decision: Decision) {
        run.record.decision = Some(decision.clone());
        if decision.is_confident() {
            trace_log!(intent = decision.intent.label(), "the decision passed the gate");
            return self.run_intent(run, decision.intent, ToolChoice::Confident);
        }
        if decision.intent == Intent::None {
            trace_log!("the likeliest intent is none, nothing is offered");
            return self.not_recognized(run);
        }
        let choices: Vec<Choice> = decision
            .ranked()
            .into_iter()
            .filter(|(intent, _)| !matches!(intent, Intent::Cancel | Intent::None))
            .take(3)
            .map(|(intent, p)| Choice { intent, percent: (p * 100.).round().clamp(0., 100.) as u8 })
            .collect();
        trace_log!(
            choices = %choices.iter().map(|c| format!("{}={}%", c.intent.label(), c.percent)).collect::<Vec<_>>().join(" "),
            "the decision did not pass the gate, the user is asked"
        );
        self.show_window(run, Phase::Choosing, HudState::Choosing { choices });
    }

    /// Nothing runs; the run is recorded as closed, not as a failure.
    fn not_recognized(&mut self, run: CommandRun) {
        self.finish_run(run, CommandOutcome::Closed, None);
        self.show_not_recognized();
    }

    fn run_intent(&mut self, mut run: CommandRun, intent: Intent, choice: ToolChoice) {
        run.record.choice = Some(choice);
        if intent == Intent::Cancel {
            trace_log!(choice = ?choice, "the cancel intent closes the command");
            self.finish_run(run, CommandOutcome::Cancelled, None);
            return self.go_idle();
        }
        if intent == Intent::None {
            trace_log!(choice = ?choice, "not a command, no tool runs");
            return self.not_recognized(run);
        }
        let selected = run.selected_text().map(str::to_owned);
        trace_log!(
            tool = intent.label(),
            why = ?choice,
            selection_chars = selected.as_ref().map(|text| text.chars().count()),
            "tool chosen"
        );
        if needs_text(intent) && selected.is_none() {
            trace_log!(tool = intent.label(), "the tool needs text and nothing is selected");
            return self.show_window(run, Phase::NeedsText { intent }, HudState::NeedsText);
        }
        let from_selection = selected.is_some();
        self.run_tool(run, intent, selected, from_selection);
    }

    /// `from_selection`: `text` is the selection, not text the user entered. A reply never replaces it.
    fn run_tool(&mut self, mut run: CommandRun, intent: Intent, text: Option<String>, from_selection: bool) {
        let write = match (intent, from_selection) {
            (Intent::Reply, _) => ResultWrite::CopyOnly,
            // A question is answered in a window and never writes, so its selection is not judged.
            (Intent::Ask, true) => ResultWrite::ReplaceSelection,
            (_, true) if !run.selection_editable() => {
                trace_log!(tool = intent.label(), "the selection cannot be edited, the result can only be copied");
                ResultWrite::CopyOnly
            }
            (_, true) => ResultWrite::ReplaceSelection,
            (_, false) => ResultWrite::InsertAtCursor,
        };
        // Only a tool the decision itself chose takes its rewrite op: a tool picked in the choice window or by the
        // polish button was not decided by it.
        let hint = run.record.decision.as_ref().filter(|_| run.record.choice == Some(ToolChoice::Confident));
        let hint = hint.and_then(|decision| decision.rewrite_op);
        // The language the speech engine reported for a spoken instruction, else what its script suggests.
        let lang = pipeline::transcript_lang(run.record.lang.as_deref(), &run.instruction);
        let heard = Heard { lang, rewrite_op: hint };
        let Some(tool) = Tool::new(intent, run.instruction.clone(), text.clone(), heard) else {
            return self.command_error(run, ProviderError::new(ErrorKind::Other, "no tool for this intent"));
        };
        let surrounding = if tool.takes_surrounding() { run.fitted_surrounding(text.as_deref()) } else { None };
        let cfg = self.deps.config.get();
        run.record.tool = Some(intent);
        run.record.polish_engine = Some(cfg.text_model.label());
        run.record.source =
            text.map(|text| (if from_selection { SourceKind::Selection } else { SourceKind::Entered }, text));
        trace_log!(engine = ?run.record.polish_engine, rewrite_hint = ?hint, "tool runs");
        let style = run.target.clone();
        self.start_step(run, HudState::Working, move |env| tool_step(env, intent, write, tool, style, surrounding));
    }

    /// A question is answered in a window and never replaces anything. A result that can only be copied (a reply, or
    /// anything made from a selection that cannot be edited) and a summary always wait in the confirm window: a summary
    /// is never put over the text it summarizes without asking. The other tools replace at once, unless the setting
    /// asks first.
    fn finish_tool(&mut self, mut run: CommandRun, intent: Intent, write: ResultWrite, text: String) {
        run.record.result = Some(text.clone());
        if intent == Intent::Ask {
            trace_log!(answer = ?text, "the answer is shown in a window");
            return self.show_window(run, Phase::Answer, HudState::Answer { text });
        }
        let confirm_setting = self.deps.config.get().general.confirm_before_replace;
        let always_confirm = write == ResultWrite::CopyOnly || intent == Intent::Summarize;
        if confirm_setting || always_confirm {
            trace_log!(confirm_setting, always_confirm, "the result waits for confirmation");
            let hud = HudState::Confirm { text, write };
            return self.show_window(run, Phase::Confirm { write }, hud);
        }
        self.deliver_command(run, text, write == ResultWrite::ReplaceSelection);
    }

    /// Writes over the selection, or inserts at the cursor when there was none. Both block, so both run on the
    /// blocking pool; the outcome comes back as `Msg::Delivered`.
    fn deliver_command(&mut self, mut run: CommandRun, text: String, replaces_selection: bool) {
        self.epoch += 1;
        let method = self.deps.config.get().general.insert_method;
        let selection = run.selection.clone().filter(|_| replaces_selection);
        run.record.result = Some(text.clone());
        trace_log!(?method, replaces_selection, chars = text.chars().count(), "delivering the result");
        match selection {
            Some(selection) => self.start_delivery(move || selection.replace(&text, method), true, Some(run)),
            None => {
                let sink = self.deps.env.sink.clone();
                self.start_delivery(move || sink.deliver(&text, method), false, Some(run));
            }
        }
        self.set_hud(HudState::Working);
    }

    fn command_error(&mut self, run: CommandRun, e: ProviderError) {
        trace_log!(kind = ?e.kind, error = %e, "the command failed");
        // Nothing was said, so nothing is kept, as with a dictation.
        if e.kind != ErrorKind::NoSpeech {
            self.finish_run(run, CommandOutcome::Failed, Some(e.to_string()));
        }
        self.epoch += 1;
        let action = match e.kind {
            ErrorKind::Mic | ErrorKind::Auth | ErrorKind::Unavailable => ErrorAction::OpenSettings,
            _ => ErrorAction::CloseOnly,
        };
        self.show_error(e.kind, e.message, action, None);
    }

    /// The outcome of the delivery that ended a run.
    pub(super) fn finish_delivered_command(&self, run: CommandRun, delivery: &Delivery, replaced: bool) {
        match delivery {
            Delivery::Inserted if replaced => self.finish_run(run, CommandOutcome::Replaced, None),
            Delivery::Inserted => self.finish_run(run, CommandOutcome::Inserted, None),
            Delivery::Copied => self.finish_run(run, CommandOutcome::Copied, None),
            Delivery::Failed(detail) => self.finish_run(run, CommandOutcome::Failed, Some(detail.clone())),
        }
    }

    /// Queues the history row of the run.
    fn finish_run(&self, run: CommandRun, outcome: CommandOutcome, error: Option<String>) {
        let CommandRun { selection, target: style, instruction, record, .. } = run;
        let source = record.source.or_else(|| {
            selection
                .filter(|selection| !selection.text().is_empty())
                .map(|selection| (SourceKind::Selection, selection.text().to_owned()))
        });
        let (source_kind, source_text) = match source {
            Some((kind, text)) => (Some(kind), Some(text)),
            None => (None, None),
        };
        let basis =
            if instruction.trim().is_empty() { source_text.as_deref().unwrap_or_default() } else { &instruction };
        let lang = record.lang.unwrap_or_else(|| {
            if basis.trim().is_empty() { String::new() } else { detect_lang(basis).code().to_string() }
        });
        trace_log!(
            outcome = ?outcome,
            error = ?error,
            asr_ms = ?record.asr_ms,
            route_ms = ?record.route_ms,
            tool_ms = ?record.tool_ms,
            "history row queued"
        );
        let entry = NewHistory {
            ts: record.ts,
            kind: HistoryKind::Command,
            style,
            app: record.app,
            lang,
            duration_ms: record.recording_ms,
            raw: instruction,
            polished: record.result,
            error,
            audio_path: None,
            engines: EngineLabels { asr: record.asr_engine, polish: record.polish_engine },
            timings: StepMillis { asr: record.asr_ms, polish: record.tool_ms },
            source: source_text,
            command: Some(CommandDetails {
                origin: record.origin,
                decision: record.decision,
                choice: record.choice,
                tool: record.tool,
                source: source_kind,
                outcome,
                route_ms: record.route_ms,
                router_engine: record.router_engine,
            }),
        };
        self.history.save_entry(entry);
    }

    fn show_window(&mut self, run: CommandRun, phase: Phase, hud: HudState) {
        self.epoch += 1;
        self.state = State::Command(CommandState { run, phase });
        trace_log!(window = self.state.trace_name(), hud = ?hud, "window shown");
        self.set_hud(hud);
    }

    /// Takes the run out of the command state when its phase is one `accepts`; otherwise leaves everything as it is.
    fn take_command(&mut self, accepts: fn(&Phase) -> bool) -> Option<(CommandRun, Phase)> {
        if !matches!(&self.state, State::Command(state) if accepts(&state.phase)) {
            return None;
        }
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Command(CommandState { run, phase }) => Some((run, phase)),
            other => {
                self.state = other;
                None
            }
        }
    }

    fn start_step<F>(&mut self, run: CommandRun, hud: HudState, step: impl FnOnce(StepEnv) -> F)
    where
        F: Future<Output = StepResult> + Send + 'static,
    {
        self.epoch += 1;
        let cancel = CancellationToken::new();
        let env = StepEnv {
            engines: self.deps.engines.clone(),
            cfg: self.deps.config.get(),
            store: self.deps.store.clone(),
            cancel: cancel.clone(),
        };
        let (tx, epoch, step) = (self.tx.clone(), self.epoch, step(env));
        tokio::spawn(
            async move {
                let _ = tx.send(Msg::Step(epoch, step.await));
            }
            .instrument(self.span.clone()),
        );
        trace_log!(hud = ?hud, "step started");
        self.set_hud(hud);
        self.state = State::Command(CommandState { run, phase: Phase::Working { cancel } });
    }
}
