use std::time::{Duration, Instant};

use reqwest::redirect::Policy;
use reqwest::{Client, Url};

use crate::trace_log;

/// A connection that is not open by then is left to the request it was meant for.
const PRECONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Leaves an open connection to the host of `endpoint` in the pool of `client`, so the next request to that host
/// skips the name lookup and the TCP and TLS handshakes. reqwest opens a connection only for a request, so this sends
/// the smallest one there is: `HEAD /`, with no key, query or body. Its answer is ignored and so is its failure, after
/// which the next request connects by itself.
pub(crate) async fn preconnect(client: &Client, endpoint: &str) {
    let Ok(endpoint) = Url::parse(endpoint.trim()) else { return };
    let host_root = endpoint.origin().ascii_serialization();
    let started = Instant::now();
    let answer = client.head(host_root).timeout(PRECONNECT_TIMEOUT).send().await;
    let ms = started.elapsed().as_millis() as u64;
    match answer {
        Ok(response) => trace_log!(ms, status = response.status().as_u16(), "connection opened ahead of the request"),
        Err(error) => trace_log!(ms, error = %error.without_url(), "no connection opened ahead of the request"),
    }
}

/// reqwest's own redirect rules, except that [`preconnect`] stays on the host it was given: an engine's requests never
/// ask for the root of the host, so a redirect from there can only be the warm-up's.
pub(crate) fn redirects_except_for_preconnect() -> Policy {
    let usual = Policy::default();
    Policy::custom(move |attempt| {
        if attempt.previous().first().is_some_and(|asked| asked.path() == "/") {
            attempt.stop()
        } else {
            usual.redirect(attempt)
        }
    })
}
