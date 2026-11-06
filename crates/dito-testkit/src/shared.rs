use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::behavior::Behavior;
use crate::recorded::Recorded;

#[derive(Default)]
pub(crate) struct Shared {
    behavior: Mutex<Behavior>,
    requests: Mutex<Vec<Recorded>>,
    files: Mutex<HashMap<String, Vec<u8>>>,
}

pub(crate) type SharedState = State<Arc<Shared>>;

impl Shared {
    pub(crate) fn update_behavior(&self, f: impl FnOnce(&mut Behavior)) {
        f(&mut self.behavior.lock().expect("behavior mutex poisoned"));
    }

    pub(crate) fn behavior(&self) -> Behavior {
        self.behavior.lock().expect("behavior mutex poisoned").clone()
    }

    pub(crate) fn record(&self, request: Recorded) {
        self.requests.lock().expect("requests mutex poisoned").push(request);
    }

    pub(crate) fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().expect("requests mutex poisoned").clone()
    }

    pub(crate) fn add_file(&self, name: &str, bytes: Vec<u8>) {
        self.files.lock().expect("files mutex poisoned").insert(name.into(), bytes);
    }

    pub(crate) fn file(&self, name: &str) -> Option<Vec<u8>> {
        self.files.lock().expect("files mutex poisoned").get(name).cloned()
    }

    /// Consumes the one-shot download cut and returns it with the per-chunk delay.
    pub(crate) fn take_download_plan(&self) -> (Option<usize>, Duration) {
        let mut b = self.behavior.lock().expect("behavior mutex poisoned");
        (b.cut_download_after.take(), b.download_chunk_delay)
    }

    /// Common gate: latency, forced failure, and key check. Returns an error response to send, if any.
    pub(crate) async fn gate(&self, headers: &HeaderMap) -> Option<Response> {
        let b = self.behavior();
        if !b.latency.is_zero() {
            tokio::time::sleep(b.latency).await;
        }
        if let Some(code) = b.fail_status {
            let status = StatusCode::from_u16(code).expect("fail_status is a valid HTTP status");
            return Some((status, Json(json!({"error": {"message": "forced failure"}}))).into_response());
        }
        let key = b.expected_key?;
        let ok = headers.get("api-key").and_then(|v| v.to_str().ok()) == Some(key.as_str())
            || headers.get("authorization").and_then(|v| v.to_str().ok()) == Some(format!("Bearer {key}").as_str());
        if ok {
            return None;
        }
        Some((StatusCode::UNAUTHORIZED, Json(json!({"error": {"message": "invalid key"}}))).into_response())
    }
}
