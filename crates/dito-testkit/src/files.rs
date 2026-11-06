use std::io;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::sync::mpsc;

use crate::recorded::{Recorded, header_pairs};
use crate::shared::SharedState;

const CHUNK_SIZE: usize = 16 * 1024;

pub(crate) async fn download(State(s): SharedState, headers: HeaderMap, Path(name): Path<String>) -> Response {
    let Some(data) = s.file(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let total = data.len();
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split('-').next())
        .and_then(|v| v.parse::<usize>().ok());
    s.record(Recorded { path: format!("files/{name}"), headers: header_pairs(&headers), ..Default::default() });
    let start = range.unwrap_or(0);
    if start > total {
        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    }
    let (cut, delay) = s.take_download_plan();
    let mut body = data[start..].to_vec();
    let truncated = cut.is_some_and(|c| c < body.len());
    if let Some(c) = cut {
        body.truncate(c);
    }
    let declared = total - start;
    let mut resp = Response::new(Body::from_stream(chunk_stream(body, delay, truncated)));
    *resp.status_mut() = if range.is_some() { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };
    let h = resp.headers_mut();
    h.insert("content-length", HeaderValue::from(declared));
    if range.is_some() {
        let content_range = format!("bytes {start}-{}/{total}", total - 1);
        h.insert("content-range", HeaderValue::from_str(&content_range).expect("ASCII header value"));
    }
    h.insert("accept-ranges", HeaderValue::from_static("bytes"));
    resp
}

/// Streams `body` in chunks with an optional per-chunk delay. When `truncated`, the stream ends with an I/O error so
/// the client sees a broken download rather than a short one.
fn chunk_stream(body: Vec<u8>, delay: Duration, truncated: bool) -> ReceiverStream {
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        let chunks = body.chunks(CHUNK_SIZE).map(|c| Ok(Bytes::copy_from_slice(c)));
        let cut = truncated.then(|| Err(io::Error::new(io::ErrorKind::ConnectionReset, "cut")));
        for item in chunks.chain(cut) {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if tx.send(item).await.is_err() {
                return;
            }
        }
    });
    ReceiverStream(rx)
}

struct ReceiverStream(mpsc::Receiver<io::Result<Bytes>>);

impl futures_core::Stream for ReceiverStream {
    type Item = io::Result<Bytes>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}
