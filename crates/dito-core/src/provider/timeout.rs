use std::time::Duration;

pub(crate) fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

/// How often a streaming call looks at its cancel flag while it waits for the server.
pub(crate) const CANCEL_POLL: Duration = Duration::from_millis(25);
