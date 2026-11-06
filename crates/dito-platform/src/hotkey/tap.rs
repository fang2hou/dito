//! `CGEventTap` source feeding the shortcut [`Machine`]. An active (event-suppressing) session
//! tap needs Accessibility; Input Monitoring is only for listen-only taps, which we never use.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes, kCFRunLoopDefaultMode};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
    CallbackResult, EventField,
};

use super::{Action, Machine, RawEvent, Shortcut};

#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("the event tap could not be created; Accessibility must be allowed")]
    PermissionDenied,
    #[error("hotkey thread failed to start: {0}")]
    Thread(String),
}

/// Handle to the running tap. Dropping it stops the thread.
pub struct HotkeyService {
    machine: Arc<Mutex<Machine>>,
    stop: Arc<AtomicBool>,
}

impl HotkeyService {
    /// Start the tap on its own thread. Actions arrive on the returned receiver.
    pub fn start(shortcut: Shortcut) -> Result<(Self, Receiver<Action>), HotkeyError> {
        let machine = Arc::new(Mutex::new(Machine::new(shortcut)));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<Action>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), HotkeyError>>();
        let (m2, s2) = (machine.clone(), stop.clone());
        std::thread::Builder::new()
            .name("dito-hotkey".into())
            .spawn(move || run_tap(m2, s2, tx, ready_tx))
            .map_err(|e| HotkeyError::Thread(e.to_string()))?;
        ready_rx.recv_timeout(Duration::from_secs(5)).map_err(|_| HotkeyError::Thread("timed out".into()))??;
        Ok((Self { machine, stop }, rx))
    }

    pub fn set_shortcut(&self, s: Shortcut) {
        self.machine.lock().expect("hotkey machine lock").set_shortcut(s);
    }

    /// The next shortcut the user presses is reported as [`Action::Captured`]; Esc reports `CaptureCancelled`.
    pub fn begin_capture(&self) {
        self.machine.lock().expect("hotkey machine lock").begin_capture();
    }

    /// Stop waiting for a shortcut without changing anything.
    pub fn cancel_capture(&self) {
        self.machine.lock().expect("hotkey machine lock").cancel_capture();
    }

    /// Consume Esc while a dictation is on screen, so it does not also reach the frontmost app.
    pub fn set_swallow_escape(&self, on: bool) {
        self.machine.lock().expect("hotkey machine lock").swallow_escape = on;
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn run_tap(
    machine: Arc<Mutex<Machine>>,
    stop: Arc<AtomicBool>,
    tx: Sender<Action>,
    ready: Sender<Result<(), HotkeyError>>,
) {
    let reenable = Arc::new(AtomicBool::new(false));
    let callback = tap_callback(machine, tx, reenable.clone());
    let tap = match CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::KeyDown, CGEventType::KeyUp, CGEventType::FlagsChanged],
        callback,
    ) {
        Ok(t) => t,
        Err(()) => {
            let _ = ready.send(Err(HotkeyError::PermissionDenied));
            return;
        }
    };
    let Ok(source) = tap.mach_port().create_runloop_source(0) else {
        let _ = ready.send(Err(HotkeyError::Thread("no run loop source".into())));
        return;
    };
    let rl = CFRunLoop::get_current();
    // SAFETY: the mode constant is a CoreFoundation global valid for the process lifetime, and `source` is live.
    unsafe { rl.add_source(&source, kCFRunLoopCommonModes) };
    tap.enable();
    let _ = ready.send(Ok(()));

    pump_run_loop(&tap, &stop, &reenable);
}

fn tap_callback(
    machine: Arc<Mutex<Machine>>,
    tx: Sender<Action>,
    reenable: Arc<AtomicBool>,
) -> impl Fn(CGEventTapProxy, CGEventType, &CGEvent) -> CallbackResult + 'static {
    move |_proxy, etype, event| {
        let code = || event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
        let flags = event.get_flags().bits();
        let raw = match etype {
            CGEventType::FlagsChanged => RawEvent::FlagsChanged { keycode: code(), flags },
            CGEventType::KeyDown => RawEvent::KeyDown {
                keycode: code(),
                flags,
                repeat: event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0,
            },
            CGEventType::KeyUp => RawEvent::KeyUp { keycode: code(), flags },
            CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                reenable.store(true, Ordering::SeqCst);
                return CallbackResult::Keep;
            }
            _ => return CallbackResult::Keep,
        };
        let out = machine.lock().expect("hotkey machine lock").feed(raw, Instant::now());
        for a in out.actions {
            let _ = tx.send(a);
        }
        if out.swallow { CallbackResult::Drop } else { CallbackResult::Keep }
    }
}

fn pump_run_loop(tap: &CGEventTap, stop: &AtomicBool, reenable: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        // SAFETY: the mode constant is a CoreFoundation global valid for the process lifetime.
        unsafe { CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_millis(500), false) };
        // The system disables a slow tap; turn it back on.
        if reenable.swap(false, Ordering::SeqCst) {
            tap.enable();
        }
    }
}
