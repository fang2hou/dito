//! Dictation into Dito's own text fields. The platform sink types only into other apps, because Dito's windows publish
//! no accessibility text; a field of the Tools or Settings window takes the text as a paste from inside the app.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use dito_core::config::InsertMethod;
use dito_core::controller::{Delivery, Sink};
use dito_platform::insert::ClipboardSwap;
use gpui_kit::base::input::Paste;
use gpui_kit::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::state::AppState;

/// How long delivery waits for the window to answer before it falls back to the platform sink.
const ANSWER_WAIT: Duration = Duration::from_millis(500);
/// The field reads the clipboard after the paste action runs; the user's clipboard goes back after this.
const CLIPBOARD_BACK: Duration = Duration::from_millis(400);

struct Request {
    text: String,
    /// Whoever sets it first owns the text: the window that pastes it, or the sink that stopped waiting.
    claimed: Arc<AtomicBool>,
    done: mpsc::SyncSender<bool>,
}

struct OwnWindowSink {
    inner: Arc<dyn Sink>,
    requests: UnboundedSender<Request>,
}

/// The requests the window side answers, taken once by [`serve_own_window_inserts`].
pub struct OwnWindowInserts(Mutex<Option<UnboundedReceiver<Request>>>);

pub fn own_window_sink(inner: Arc<dyn Sink>) -> (Arc<dyn Sink>, OwnWindowInserts) {
    let (requests, received) = unbounded_channel();
    (Arc::new(OwnWindowSink { inner, requests }), OwnWindowInserts(Mutex::new(Some(received))))
}

impl Sink for OwnWindowSink {
    fn deliver(&self, text: &str, method: InsertMethod) -> Delivery {
        let (done, answer) = mpsc::sync_channel(1);
        let claimed = Arc::new(AtomicBool::new(false));
        let request = Request { text: text.to_owned(), claimed: claimed.clone(), done };
        if self.requests.send(request).is_ok() {
            match answer.recv_timeout(ANSWER_WAIT) {
                Ok(true) => return Delivery::Inserted,
                Ok(false) => {}
                // A window that claimed the text in the meantime has pasted it.
                Err(_) if claimed.swap(true, Ordering::SeqCst) => return Delivery::Inserted,
                Err(_) => {}
            }
        }
        self.inner.deliver(text, method)
    }

    fn copy(&self, text: &str) -> Delivery {
        self.inner.copy(text)
    }
}

pub fn serve_own_window_inserts(st: &AppState, cx: &mut App) {
    let Some(mut requests) = st.own_inserts.0.lock().ok().and_then(|mut slot| slot.take()) else { return };
    cx.spawn(async move |cx| {
        while let Some(request) = requests.recv().await {
            let inserted = cx.update(|cx| paste_into_focused_field(&request, cx));
            let _ = request.done.send(inserted);
        }
    })
    .detach();
}

/// The HUD's own editors are left out: a command's result goes back to the app it was started from.
fn paste_into_focused_field(request: &Request, cx: &mut App) -> bool {
    if crate::onboarding::insert_try(&request.text, &request.claimed, cx) {
        return true;
    }
    let windows = [
        crate::tools::handle(cx).map(|(window, _)| AnyWindowHandle::from(window)),
        crate::settings::handle(cx).map(|(window, _)| AnyWindowHandle::from(window)),
    ];
    for window in windows.into_iter().flatten() {
        let pasted = window.update(cx, |_, window, cx| {
            if !window.is_window_active()
                || !window.is_action_available(&Paste, cx)
                || request.claimed.swap(true, Ordering::SeqCst)
            {
                return false;
            }
            let swap = match ClipboardSwap::begin(&request.text) {
                Ok(swap) => swap,
                Err(e) => {
                    tracing::warn!("insert: could not put the text on the clipboard for Dito's own field: {e}");
                    request.claimed.store(false, Ordering::SeqCst);
                    return false;
                }
            };
            window.dispatch_action(Box::new(Paste), cx);
            cx.spawn(async move |cx| {
                cx.background_executor().timer(CLIPBOARD_BACK).await;
                swap.restore();
            })
            .detach();
            true
        });
        if pasted.unwrap_or(false) {
            return true;
        }
    }
    false
}
