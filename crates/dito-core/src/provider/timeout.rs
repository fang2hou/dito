use std::time::Duration;

use super::ChatCancel;
use crate::error::{ErrorKind, ProviderError, ProviderResult};

pub(crate) fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

/// How often a streaming call looks at its cancel flag while it waits for the server.
const CANCEL_POLL: Duration = Duration::from_millis(25);

/// The next item of a stream. A raised cancel flag ends the wait at once as the end of the stream; a server silent
/// for the idle timeout is an error.
pub(crate) async fn next_stream_item<T>(
    next: impl Future<Output = ProviderResult<Option<T>>>,
    cancel: &ChatCancel,
) -> ProviderResult<Option<T>> {
    let idle = tokio::time::sleep(default_timeout());
    tokio::pin!(next, idle);
    let mut tick = tokio::time::interval(CANCEL_POLL);
    loop {
        tokio::select! {
            item = &mut next => return item,
            _ = tick.tick() => {
                if cancel.is_cancelled() {
                    return Ok(None);
                }
            }
            () = &mut idle => return Err(ProviderError::new(ErrorKind::Timeout, "the stream went silent")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_stream_that_goes_silent_fails_instead_of_hanging() {
        let silent = std::future::pending::<ProviderResult<Option<()>>>();
        let error = next_stream_item(silent, &ChatCancel::default()).await.unwrap_err();
        assert_eq!((error.kind, error.message.as_str()), (ErrorKind::Timeout, "the stream went silent"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancel_raised_during_the_wait_ends_the_stream_within_one_poll() {
        let cancel = ChatCancel::default();
        let stop = cancel.clone();
        let raised_after = Duration::from_secs(1);
        tokio::spawn(async move {
            tokio::time::sleep(raised_after).await;
            stop.cancel();
        });
        let started = tokio::time::Instant::now();
        let silent = std::future::pending::<ProviderResult<Option<()>>>();
        assert_eq!(next_stream_item(silent, &cancel).await, Ok(None));
        assert!(started.elapsed() <= raised_after + CANCEL_POLL, "waited {:?}", started.elapsed());
    }
}
