use std::time::Instant;

use tokio::sync::{mpsc, watch};

use super::command::Command;
use super::env::Delivery;
use super::hud::HudState;
use super::progress::ProcessingEstimate;
use crate::pipeline::{Output, PipelineError, PipelinePhase};

pub(super) enum Msg {
    Cmd(Command),
    PipelinePhase(u64, PipelinePhase),
    Done(u64, Result<Output, PipelineError>),
    /// The insertion that `Done` started on the blocking pool has finished.
    Delivered(u64, Delivery),
    Hide(u64, u64),
    RecordingLimit(u64),
}

#[derive(Clone)]
pub struct DictationHandle {
    pub(super) tx: mpsc::UnboundedSender<Msg>,
    hud: watch::Receiver<HudState>,
    recording_started: watch::Receiver<Option<Instant>>,
    processing: watch::Receiver<ProcessingEstimate>,
}

impl DictationHandle {
    pub(super) fn new(
        tx: mpsc::UnboundedSender<Msg>,
        hud: watch::Receiver<HudState>,
        recording_started: watch::Receiver<Option<Instant>>,
        processing: watch::Receiver<ProcessingEstimate>,
    ) -> Self {
        Self { tx, hud, recording_started, processing }
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(Msg::Cmd(c));
    }

    pub fn hud(&self) -> watch::Receiver<HudState> {
        self.hud.clone()
    }

    pub fn recording_started(&self) -> Option<Instant> {
        *self.recording_started.borrow()
    }

    pub fn processing_estimate(&self) -> ProcessingEstimate {
        *self.processing.borrow()
    }
}
