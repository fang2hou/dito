use std::time::Duration;

pub(crate) fn default_timeout() -> Duration {
    Duration::from_secs(30)
}
