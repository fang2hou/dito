use std::time::Duration;

use dito_core::config::InsertMethod;
use dito_core::controller::{Delivery, Sink};
use dito_core::insert::{Flow, Next, Outcome, Step};

use super::focus::{FocusTarget, own_pid, wait_for_stable_focus};
use super::keyboard::post_type;
use super::pasteboard::{PasteGuard, begin_paste, copy_to_clipboard};
use super::verify::verify;

/// The user's clipboard goes back no sooner than this after a paste, and only once the insert
/// has been confirmed (apps read the pasteboard asynchronously, when they get to the Cmd+V).
const RESTORE_FLOOR: Duration = Duration::from_millis(500);
/// When verification cannot confirm the insert (e.g. secure fields), give slow apps longer.
const RESTORE_FLOOR_UNVERIFIED: Duration = Duration::from_millis(1000);

/// The real text sink: insert at the cursor when possible, otherwise leave the text on the clipboard.
pub struct MacSink;

impl Sink for MacSink {
    fn deliver(&self, text: &str, method: InsertMethod) -> Delivery {
        deliver_text(text, method)
    }
}

fn terminal_clipboard(text: &str) -> Delivery {
    match copy_to_clipboard(text) {
        Ok(()) => Delivery::Copied,
        Err(e) => {
            tracing::warn!("insert: clipboard copy failed: {e}");
            Delivery::Failed(e.to_string())
        }
    }
}

/// The focused element live insertion can go to, or `None` (after logging why) when the text
/// should be left on the clipboard instead.
fn deliverable_target() -> Option<FocusTarget> {
    let Some(target) = wait_for_stable_focus() else {
        tracing::warn!("insert: no readable focused element (Accessibility permission?); text left on clipboard");
        return None;
    };
    if target.pid == own_pid() {
        tracing::warn!(
            "insert: keyboard focus is on dito itself (role {:?}); a HUD/settings click moved it; \
             text left on the clipboard",
            target.role
        );
        return None;
    }
    if target.editable {
        return Some(target);
    }
    if !target.foreign_key_window() {
        tracing::warn!(
            "insert: focused element (pid {}, role {:?}) takes no text; text left on clipboard",
            target.pid,
            target.role
        );
        return None;
    }
    tracing::info!(
        "insert: focus is the app's key window (pid {}, {:?}) with no AX text tree; inserting without verification",
        target.pid,
        target.title
    );
    Some(target)
}

/// What to do with the saved clipboard once an attempt is over.
enum Restore {
    InBackground(Duration),
    Now(Duration),
}

impl Restore {
    fn apply(self, guard: Option<PasteGuard>) {
        let Some(guard) = guard else { return };
        match self {
            Self::InBackground(floor) => guard.restore_after(floor),
            Self::Now(floor) => guard.restore_blocking(floor),
        }
    }
}

/// One delivery in progress: the text as UTF-16, where the cursor was, and the clipboard to give back.
struct Attempts<'a> {
    text: &'a str,
    target: &'a FocusTarget,
    units: Vec<u16>,
    baseline: Option<usize>,
    guard: Option<PasteGuard>,
}

impl Attempts<'_> {
    /// Runs one live attempt and verifies it. Returns the verdict and the last selection location seen.
    fn run(&mut self, step: Step) -> (Outcome, Option<usize>) {
        let result = match step {
            Step::Type { from } => self.type_suffix(from),
            Step::Paste => self.paste(),
        };
        if let (Outcome::Refuted { inserted }, expected) = (result.0, self.expected_units(step)) {
            tracing::warn!("insert: {step:?} attempt refuted; {inserted}/{expected} units landed");
        }
        result
    }

    fn type_suffix(&mut self, from: usize) -> (Outcome, Option<usize>) {
        let from = from.min(self.units.len());
        let suffix = &self.units[from..];
        tracing::debug!("insert: typing {} units from offset {from}", suffix.len());
        if !post_type(suffix) {
            tracing::warn!("insert: could not post key events; nothing typed");
            return (Outcome::Refuted { inserted: 0 }, self.baseline);
        }
        verify(self.target, suffix.len(), self.baseline, &String::from_utf16_lossy(suffix))
    }

    fn paste(&mut self) -> (Outcome, Option<usize>) {
        match begin_paste(self.text) {
            Ok(guard) => {
                self.guard = Some(guard);
                verify(self.target, self.units.len(), self.baseline, self.text)
            }
            Err(e) => {
                tracing::warn!("insert: paste setup failed: {e}");
                (Outcome::Refuted { inserted: 0 }, self.baseline)
            }
        }
    }

    fn expected_units(&self, step: Step) -> usize {
        match step {
            Step::Type { from } => self.units.len().saturating_sub(from.min(self.units.len())),
            Step::Paste => self.units.len(),
        }
    }

    fn restore(&mut self, how: Restore) {
        how.apply(self.guard.take());
    }
}

/// The delivery spine: wait for stable focus, then type/paste with verification and the
/// [`dito_core::insert`] fallback flow, ending on the clipboard if nothing live worked.
fn deliver_text(text: &str, method: InsertMethod) -> Delivery {
    let Some(target) = deliverable_target() else { return terminal_clipboard(text) };
    let mut attempts =
        Attempts { text, target: &target, units: text.encode_utf16().collect(), baseline: target.cursor, guard: None };
    let mut flow = Flow::new(method);
    let mut step = Flow::first(method);
    loop {
        let (result, last) = attempts.run(step);
        match flow.advance(step, result) {
            Next::Done => {
                if matches!(result, Outcome::Unknown) {
                    tracing::info!("insert: accepted without verification (selection unreadable)");
                }
                let floor = if matches!(result, Outcome::Confirmed) { RESTORE_FLOOR } else { RESTORE_FLOOR_UNVERIFIED };
                attempts.restore(Restore::InBackground(floor));
                return Delivery::Inserted;
            }
            Next::Attempt(next) => {
                if !super::focus::read_focus().is_some_and(|current| target.same_identity(&current)) {
                    tracing::warn!("insert: focus changed before retry; text left on clipboard");
                    attempts.restore(Restore::Now(RESTORE_FLOOR));
                    return terminal_clipboard(text);
                }
                tracing::warn!("insert: retrying with {next:?}");
                attempts.restore(Restore::InBackground(RESTORE_FLOOR));
                attempts.baseline = last.or(attempts.baseline);
                step = next;
            }
            Next::CopyToClipboard => {
                tracing::warn!("insert: live insertion failed after retries; text left on clipboard");
                attempts.restore(Restore::Now(RESTORE_FLOOR));
                return terminal_clipboard(text);
            }
        }
    }
}
