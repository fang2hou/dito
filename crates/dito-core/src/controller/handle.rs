use std::time::Instant;

use tokio::sync::{mpsc, watch};

use super::command::Command;
use super::env::Delivery;
use super::hud::{HudState, ResultWrite, RunKind};
use super::progress::{ProcessingEstimate, StepTimes};
use crate::error::ProviderResult;
use crate::intent::{Decision, Intent};
use crate::pipeline::{Output, PipelineError, PipelinePhase};

pub(super) enum Msg {
    Cmd(Command),
    PipelinePhase(u64, PipelinePhase),
    Done(u64, Result<Output, PipelineError>),
    Delivered(u64, Delivery),
    Hide(u64, u64),
    RecordingLimit(u64),
    /// The wait of a recording whose capsule is held back is over.
    RevealCapsule(u64),
    Step(u64, StepResult),
}

/// What one asynchronous step of a command run produced.
pub(super) enum StepResult {
    /// The transcript of the spoken instruction, with its language and recognition time.
    Instruction(Result<Output, PipelineError>),
    Routed {
        result: ProviderResult<Decision>,
        ms: u64,
    },
    /// What a tool produced, with what the actor needs to finish the run.
    Tool {
        intent: Intent,
        write: ResultWrite,
        result: ProviderResult<String>,
        ms: u64,
    },
}

#[derive(Clone)]
pub struct DictationHandle {
    pub(super) tx: mpsc::UnboundedSender<Msg>,
    hud: watch::Receiver<HudState>,
    run_kind: watch::Receiver<RunKind>,
    recording_started: watch::Receiver<Option<Instant>>,
    processing: watch::Receiver<ProcessingEstimate>,
    step_times: watch::Receiver<StepTimes>,
}

impl DictationHandle {
    pub(super) fn new(
        tx: mpsc::UnboundedSender<Msg>,
        hud: watch::Receiver<HudState>,
        run_kind: watch::Receiver<RunKind>,
        recording_started: watch::Receiver<Option<Instant>>,
        processing: watch::Receiver<ProcessingEstimate>,
        step_times: watch::Receiver<StepTimes>,
    ) -> Self {
        Self { tx, hud, run_kind, recording_started, processing, step_times }
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(Msg::Cmd(c));
    }

    pub fn hud(&self) -> watch::Receiver<HudState> {
        self.hud.clone()
    }

    /// Which shortcut started the current run; set before its first HUD state, and kept after it ends.
    pub fn run_kind(&self) -> RunKind {
        *self.run_kind.borrow()
    }

    /// Changes when a recording that a shortcut started turns into the other kind of run.
    pub fn run_kinds(&self) -> watch::Receiver<RunKind> {
        self.run_kind.clone()
    }

    pub fn recording_started(&self) -> Option<Instant> {
        *self.recording_started.borrow()
    }

    pub fn processing_estimate(&self) -> ProcessingEstimate {
        *self.processing.borrow()
    }

    pub fn step_times(&self) -> StepTimes {
        *self.step_times.borrow()
    }
}
