//! Bridge between the global shortcut tap and the UI: capture mode for the shortcut field, and Esc handling.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dito_core::config::{ConfigStore, General};
use dito_core::controller::{Command, DictationHandle, HudState};
use dito_platform::hotkey::{Action, command_shortcut, dictation_may_hand_over, dictation_shortcut};
use dito_platform::{HotkeyService, Shortcut};
use tokio::sync::watch;

/// How long a refused capture stays on screen.
const REFUSED_SHOWN: Duration = Duration::from_secs(3);

/// The shortcut a capture is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutTarget {
    Dictation,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Waiting(ShortcutTarget),
    /// The captured combination was the other shortcut's, so nothing was saved.
    Refused(ShortcutTarget),
}

impl CaptureState {
    pub fn is_waiting_for(self, target: ShortcutTarget) -> bool {
        self == Self::Waiting(target)
    }
}

pub struct Hotkeys {
    service: Mutex<Option<Arc<HotkeyService>>>,
    /// What `sync_command_shortcut` gave the running service: `None` before the first sync, `Some(None)` for off.
    applied_command: Mutex<Option<Option<Shortcut>>>,
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
            applied_command: Mutex::new(None),
            capture: watch::channel(CaptureState::Idle).0,
            unavailable: watch::channel(false).0,
        }
    }

    pub fn set_service(&self, s: Option<Arc<HotkeyService>>) {
        self.unavailable.send_replace(s.is_none());
        *self.service.lock().unwrap() = s;
        // A new service starts without a command shortcut.
        *self.applied_command.lock().unwrap() = None;
    }

    pub fn available(&self) -> bool {
        self.service.lock().unwrap().is_some()
    }

    pub fn begin_capture(&self, target: ShortcutTarget) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            s.begin_capture();
            self.capture.send_replace(CaptureState::Waiting(target));
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

    /// Gives the service the command shortcut the settings resolve to unless it already has it: unset follows the
    /// dictation shortcut, and none is left on when it is unusable (see `command_shortcut`).
    pub fn sync_command_shortcut(&self, general: &General) {
        let service = self.service.lock().unwrap();
        let Some(service) = service.as_ref() else { return };
        let wanted = command_shortcut(&general.shortcut, &general.command_shortcut);
        let mut applied = self.applied_command.lock().unwrap();
        if *applied == Some(wanted) {
            return;
        }
        if wanted.is_none() {
            tracing::info!("no command shortcut (saved {:?}); command mode is off", general.command_shortcut);
        }
        service.set_command_shortcut(wanted);
        *applied = Some(wanted);
    }

    /// Ends a capture that gave `shortcut`: saves it for the target and applies it to the tap at once, or refuses it
    /// when it is the other shortcut's.
    fn finish_capture(self: &Arc<Self>, shortcut: Shortcut, cfg: &ConfigStore) {
        let CaptureState::Waiting(target) = *self.capture.borrow() else { return };
        let general = cfg.get().general.clone();
        let other = match target {
            ShortcutTarget::Dictation => Shortcut::parse(&general.command_shortcut),
            ShortcutTarget::Command => Some(dictation_shortcut(&general.shortcut)),
        };
        if other == Some(shortcut) {
            self.refuse(target);
            return;
        }
        let id = shortcut.id();
        let saved = cfg.update(|c| match target {
            ShortcutTarget::Dictation => c.general.shortcut = id.clone(),
            ShortcutTarget::Command => c.general.command_shortcut = id.clone(),
        });
        if let Err(e) = saved {
            tracing::warn!("could not save shortcut: {e}");
        }
        if target == ShortcutTarget::Dictation {
            self.set_shortcut(&id);
        }
        self.sync_command_shortcut(&cfg.get().general);
        self.capture.send_replace(CaptureState::Idle);
    }

    /// Shows the refusal for a few seconds, unless another capture has started by then.
    fn refuse(self: &Arc<Self>, target: ShortcutTarget) {
        let refused = CaptureState::Refused(target);
        self.capture.send_replace(refused);
        let hotkeys = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(REFUSED_SHOWN);
            hotkeys.capture.send_if_modified(|state| {
                let shown = *state == refused;
                if shown {
                    *state = CaptureState::Idle;
                }
                shown
            });
        });
    }

    pub fn set_swallow_escape(&self, on: bool) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            s.set_swallow_escape(on);
        }
    }

    pub fn set_escape_to_ime(&self, on: bool) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            s.set_escape_to_ime(on);
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
                    Action::ShortcutDown => {
                        let config = cfg.get();
                        let general = &config.general;
                        dictation.send(if dictation_may_hand_over(&general.shortcut, &general.command_shortcut) {
                            Command::KeyDownMayHandOver
                        } else {
                            Command::KeyDown
                        })
                    }
                    Action::ShortcutUp => dictation.send(Command::KeyUp),
                    Action::CommandDown => dictation.send(Command::CommandKeyDown),
                    Action::CommandUp => dictation.send(Command::CommandKeyUp),
                    Action::ShortcutToCommand => dictation.send(Command::KeyToCommand),
                    Action::CommandToShortcut => dictation.send(Command::CommandToKey),
                    Action::ChordAbort => dictation.send(Command::ChordAbort),
                    Action::Escape => {
                        // A recording whose capsule is still held back counts as on screen.
                        if *hud.borrow() != HudState::Hidden || dictation.recording_started().is_some() {
                            dictation.send(Command::Cancel);
                        }
                    }
                    Action::Captured(sc) => hotkeys.finish_capture(sc, &cfg),
                    Action::CaptureCancelled => {
                        hotkeys.capture.send_replace(CaptureState::Idle);
                    }
                }
            }
        })
        .expect("spawn hotkey dispatcher");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(hotkeys: &Arc<Hotkeys>, cfg: &ConfigStore, target: ShortcutTarget, id: &str) {
        hotkeys.capture.send_replace(CaptureState::Waiting(target));
        hotkeys.finish_capture(Shortcut::parse(id).unwrap(), cfg);
    }

    fn saved(cfg: &ConfigStore) -> (String, String) {
        let general = cfg.get().general.clone();
        (general.shortcut, general.command_shortcut)
    }

    #[test]
    fn a_captured_command_shortcut_is_saved_for_the_command_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = ConfigStore::open(&path).unwrap();
        let hotkeys = Arc::new(Hotkeys::new());
        assert_eq!(saved(&cfg), ("RightCommand".into(), String::new()), "unset on a fresh config");

        capture(&hotkeys, &cfg, ShortcutTarget::Command, "Control+Option+K");
        assert_eq!(*hotkeys.capture.borrow(), CaptureState::Idle);
        assert_eq!(saved(&ConfigStore::open(&path).unwrap()), ("RightCommand".into(), "Control+Option+K".into()));
    }

    #[test]
    fn the_other_shortcuts_combination_is_refused_in_both_rows() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ConfigStore::open(dir.path().join("config.toml")).unwrap();
        let hotkeys = Arc::new(Hotkeys::new());

        capture(&hotkeys, &cfg, ShortcutTarget::Command, "RightCommand");
        assert_eq!(*hotkeys.capture.borrow(), CaptureState::Refused(ShortcutTarget::Command));
        assert_eq!(saved(&cfg), ("RightCommand".into(), String::new()), "nothing saved");

        capture(&hotkeys, &cfg, ShortcutTarget::Command, "RightOption");
        capture(&hotkeys, &cfg, ShortcutTarget::Dictation, "RightOption");
        assert_eq!(*hotkeys.capture.borrow(), CaptureState::Refused(ShortcutTarget::Dictation));
        assert_eq!(saved(&cfg), ("RightCommand".into(), "RightOption".into()));

        capture(&hotkeys, &cfg, ShortcutTarget::Dictation, "Fn");
        assert_eq!(saved(&cfg), ("Fn".into(), "RightOption".into()));
        assert_eq!(*hotkeys.capture.borrow(), CaptureState::Idle);
    }

    #[test]
    fn a_capture_that_ended_in_the_meantime_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ConfigStore::open(dir.path().join("config.toml")).unwrap();
        let hotkeys = Arc::new(Hotkeys::new());
        hotkeys.finish_capture(Shortcut::parse("Fn").unwrap(), &cfg);
        assert_eq!(saved(&cfg).0, "RightCommand");
    }
}
