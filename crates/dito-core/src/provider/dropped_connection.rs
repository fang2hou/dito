use std::error::Error;

use reqwest::Url;
use reqwest::retry::{self, Builder};

use crate::trace_log;

/// The retry rule of a cloud engine's client: a request whose connection is lost before its answer begins is sent
/// once more, at once. A request that picks a pooled connection which the server or the network has just closed ends
/// like that, and on a new connection it goes through.
///
/// Nothing else is sent again. A timeout and a failure to connect would only make the user wait twice for a host
/// that is down, a status is an answer, and an answer that has begun is never asked for a second time. The second
/// attempt runs inside the timeout and the cancellation of the first, and its failure is the request's failure.
///
/// The server may have received the first copy, so a speech or text request can run twice there. That is accepted:
/// these requests change nothing on the server, and the user's own Retry would send the second copy as well.
pub(super) fn resend_once(endpoint: &str) -> Builder {
    let Some(host) = Url::parse(endpoint.trim()).ok().and_then(|url| url.host_str().map(str::to_owned)) else {
        return retry::never();
    };
    retry::for_host(host).no_budget().max_retries_per_request(1).classify_fn(|attempt| {
        let lost = attempt
            .error()
            .and_then(|error| error.downcast_ref::<reqwest::Error>())
            .filter(|error| !error.is_timeout() && !error.is_connect());
        match lost {
            Some(error) => {
                trace_log!(cause = %root_cause(error), "connection lost before the answer, the request is sent once more");
                attempt.retryable()
            }
            None => attempt.success(),
        }
    })
}

/// The innermost cause, which names no URL. An outer error can, and a URL can hold a key.
fn root_cause(error: &reqwest::Error) -> String {
    let mut cause = error.source();
    while let Some(deeper) = cause.and_then(Error::source) {
        cause = Some(deeper);
    }
    cause.map(ToString::to_string).unwrap_or_default()
}
