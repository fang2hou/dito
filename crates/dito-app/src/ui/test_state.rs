use dito_core::provider::TestOutcome;

#[derive(Clone, Debug, PartialEq)]
pub enum TestState {
    Idle,
    Running,
    Ok(u64),
    Failed(String),
}

impl From<TestOutcome> for TestState {
    fn from(outcome: TestOutcome) -> Self {
        match outcome {
            TestOutcome::Ok { latency_ms } => Self::Ok(latency_ms),
            TestOutcome::Failed { kind, message } => Self::Failed(format!("{kind:?}: {message}")),
        }
    }
}
