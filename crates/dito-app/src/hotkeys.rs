//! Bridge between the global shortcut tap and the UI: capture mode for the shortcut field, and Esc handling.

use std::sync::{Arc, Mutex};

use dito_core::config::ConfigStore;
use dito_core::controller::{Command, DictationHandle, HudState};
use dito_platform::hotkey::Action;
use dito_platform::{HotkeyService, Shortcut};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Waiting,
}

pub struct Hotkeys {
    service: Mutex<Option<Arc<HotkeyService>>>,
    pub capture: watch::Sender<CaptureState>,
    /// Set when the tap could not start (missing permission).
    pub unavailable: watch::Sender<bool>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self::new()
    }
}

impl Hotkeys {
    pub fn new() -> Self {
        Self {
            service: Mutex::new(None),
            capture: watch::channel(CaptureState::Idle).0,
            unavailable: watch::channel(false).0,
        }
    }

    pub fn set_service(&self, s: Option<Arc<HotkeyService>>) {
        self.unavailable.send_replace(s.is_none());
        *self.service.lock().unwrap() = s;
    }

    pub fn available(&self) -> bool {
        self.service.lock().unwrap().is_some()
    }

    pub fn begin_capture(&self) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            s.begin_capture();
            self.capture.send_replace(CaptureState::Waiting);
        }
    }

    pub fn cancel_capture(&self) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            s.cancel_capture();
        }
        self.capture.send_replace(CaptureState::Idle);
    }

    pub fn set_shortcut(&self, id: &str) {
        if let (Some(s), Some(sc)) = (self.service.lock().unwrap().as_ref(), Shortcut::parse(id)) {
            s.set_shortcut(sc);
        }
    }

    pub fn set_swallow_escape(&self, on: bool) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            s.set_swallow_escape(on);
        }
    }
}

pub fn spawn_dispatcher(
    rx: std::sync::mpsc::Receiver<Action>,
    hotkeys: Arc<Hotkeys>,
    cfg: Arc<ConfigStore>,
    dictation: DictationHandle,
) {
    std::thread::Builder::new()
        .name("dito-hotkey-dispatch".into())
        .spawn(move || {
            let hud = dictation.hud();
            while let Ok(action) = rx.recv() {
                match action {
                    Action::ShortcutDown => dictation.send(Command::KeyDown),
                    Action::ShortcutUp => dictation.send(Command::KeyUp),
                    Action::ChordAbort => dictation.send(Command::Cancel),
                    Action::Escape => {
                        if *hud.borrow() != HudState::Hidden {
                            dictation.send(Command::Cancel);
                        }
                    }
                    Action::Captured(sc) => {
                        let id = sc.id();
                        if let Err(e) = cfg.update(|c| c.general.shortcut = id.clone()) {
                            tracing::warn!("could not save shortcut: {e}");
                        }
                        hotkeys.set_shortcut(&id);
                        hotkeys.capture.send_replace(CaptureState::Idle);
                    }
                    Action::CaptureCancelled => {
                        hotkeys.capture.send_replace(CaptureState::Idle);
                    }
                }
            }
        })
        .expect("spawn hotkey dispatcher");
}
