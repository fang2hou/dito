/// Counts the requests of one owner so that stale completions are dropped: beginning a new request, or
/// bumping, supersedes every earlier generation, and only the newest one may still write its result.
#[derive(Default)]
pub struct Requests {
    generation: u64,
}

impl Requests {
    /// Begin a new request and return its generation.
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// Invalidate every in-flight request.
    pub fn bump(&mut self) {
        self.generation += 1;
    }

    /// Whether the request that got `generation` is still the newest one.
    pub fn is_current(&self, generation: u64) -> bool {
        generation == self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::Requests;

    #[test]
    fn only_the_newest_request_may_write_its_result() {
        let mut reqs = Requests::default();
        let first = reqs.begin();
        assert!(reqs.is_current(first));
        let second = reqs.begin();
        assert!(!reqs.is_current(first), "an older request's completion must be dropped");
        assert!(reqs.is_current(second));
    }
}
