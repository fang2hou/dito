//! Engine health, summarized into the status dot beside the DITO wordmark.

use crate::config::{Config, Source};
use crate::factory::LocalBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    /// Engine is a provider; nothing local to run.
    Remote,
    Running,
    Loading,
    Stopping,
    Stopped,
    NotInstalled,
    Failed(String),
}

impl EngineStatus {
    pub fn is_problem(&self) -> bool {
        matches!(self, Self::Stopping | Self::Stopped | Self::NotInstalled | Self::Failed(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    pub asr: EngineStatus,
    pub polish: EngineStatus,
}

impl Health {
    pub fn compute(cfg: &Config, local: Option<&dyn LocalBackend>) -> Self {
        let status = |source: Source, model: &str| match (source, local) {
            (Source::Provider, _) => EngineStatus::Remote,
            (Source::Local, Some(l)) => l.status(model),
            (Source::Local, None) => EngineStatus::Failed("local engines are not available".into()),
        };
        Self {
            asr: status(cfg.asr.source, &cfg.asr.local_model),
            polish: if cfg.polish.enabled {
                status(cfg.text_model.source, &cfg.text_model.local_model)
            } else {
                EngineStatus::Remote
            },
        }
    }

    pub fn is_ok(&self) -> bool {
        !self.asr.is_problem() && !self.polish.is_problem()
    }

    pub fn failing_model<'a>(&self, cfg: &'a Config) -> Option<&'a str> {
        if self.asr.is_problem() {
            Some(&cfg.asr.local_model)
        } else if self.polish.is_problem() {
            Some(&cfg.text_model.local_model)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub(EngineStatus);
    #[async_trait::async_trait]
    impl LocalBackend for Stub {
        async fn transcriber(
            &self,
            _: &str,
        ) -> crate::error::ProviderResult<std::sync::Arc<dyn crate::provider::Transcriber>> {
            unimplemented!()
        }
        async fn text_model(
            &self,
            _: &str,
            _: &crate::config::LocalTextParams,
        ) -> crate::error::ProviderResult<std::sync::Arc<dyn crate::provider::TextModel>> {
            unimplemented!()
        }
        async fn router(&self, _: &str) -> crate::error::ProviderResult<std::sync::Arc<dyn crate::provider::Router>> {
            unimplemented!()
        }
        fn status(&self, _: &str) -> EngineStatus {
            self.0.clone()
        }
    }

    #[test]
    fn stopped_local_model_is_a_problem_and_names_the_model() {
        let cfg = Config::default();
        let h = Health::compute(&cfg, Some(&Stub(EngineStatus::Stopped)));
        assert!(!h.is_ok());
        assert_eq!(h.failing_model(&cfg), Some(cfg.asr.local_model.as_str()));
    }

    #[test]
    fn disabled_polish_ignores_the_polish_model() {
        let mut cfg = Config::dev_azure();
        cfg.asr.source = Source::Provider;
        cfg.polish.enabled = false;
        cfg.text_model.source = Source::Local;
        assert!(Health::compute(&cfg, Some(&Stub(EngineStatus::Stopped))).is_ok());
    }
}
