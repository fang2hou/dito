use std::time::Instant;

use tokio::sync::{mpsc, watch};

use super::command::Command;
use super::hud::HudState;
use crate::pipeline::{Output, Phase, PipelineError};

pub(super) enum Msg {
    Cmd(Command),
    Phase(u64, Phase),
    Done(u64, Result<Output, PipelineError>),
    Hide(u64, u64),
    RecordingLimit(u64),
}

#[derive(Clone)]
pub struct DictationHandle {
    pub(super) tx: mpsc::UnboundedSender<Msg>,
    hud: watch::Receiver<HudState>,
    recording_started: watch::Receiver<Option<Instant>>,
}

impl DictationHandle {
    pub(super) fn new(
        tx: mpsc::UnboundedSender<Msg>,
        hud: watch::Receiver<HudState>,
        recording_started: watch::Receiver<Option<Instant>>,
    ) -> Self {
        Self { tx, hud, recording_started }
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
}
