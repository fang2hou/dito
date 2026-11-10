//! In-memory timing estimates for the HUD, scoped to the engines used for each job.

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use crate::config::{EngineConfig, Source};
use crate::pipeline::Output;

const LIMIT: usize = 10;
const OVERHEAD_S: f32 = 0.6;
const PRIOR_WEIGHT: f32 = 3.;
const ASR_RATE: f32 = 0.25;
const POLISH_RATE: f32 = 1. / 80.;

#[derive(Clone, Copy)]
pub struct ProcessingEstimate {
    pub recognition_secs: f32,
    /// The actual polishing start and its estimated duration, including time spent during the HUD catch-up.
    pub polishing: Option<(Instant, f32)>,
}

impl Default for ProcessingEstimate {
    fn default() -> Self {
        Self { recognition_secs: OVERHEAD_S + ASR_RATE, polishing: None }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Models {
    pub asr: u64,
    pub polish: u64,
}

impl Models {
    pub fn new(asr: &EngineConfig, polish: &EngineConfig) -> Self {
        Self { asr: engine_key(asr, true), polish: engine_key(polish, false) }
    }
}

fn engine_key(engine: &EngineConfig, asr: bool) -> u64 {
    let mut key = DefaultHasher::new();
    match engine.source {
        Source::Local => ("local", &engine.local_model).hash(&mut key),
        Source::Provider => {
            if asr {
                // Cloud speech always runs the active configuration, the bundled set when none is saved.
                let configurations = engine.cloud_asr.clone().unwrap_or_default();
                ("configuration", &configurations.selected, configurations.active().to_string()).hash(&mut key);
            } else {
                // Cloud polish always runs the active configuration, the templates' when none is saved. A new
                // model or other parameters start a new estimate; the name does not.
                let active = engine.active_polish().map(|c| (c.id.clone(), format!("{:?}{:?}", c.target, c.params)));
                ("configuration", active).hash(&mut key);
            }
        }
    }
    key.finish()
}

struct Sample {
    models: Models,
    audio_secs: f32,
    asr_secs: f32,
    polish: Option<(usize, f32)>,
}

#[derive(Default)]
pub(super) struct TimingHistory(VecDeque<Sample>);

impl TimingHistory {
    pub fn recognition(&self, model: u64, audio_secs: f32) -> f32 {
        let samples = self.0.iter().filter(|s| s.models.asr == model).map(|s| (s.audio_secs.max(1.), s.asr_secs));
        estimate(ASR_RATE, audio_secs, samples)
    }

    pub fn polishing(&self, model: u64, chars: usize) -> f32 {
        let samples = self
            .0
            .iter()
            .filter(|s| s.models.polish == model)
            .filter_map(|s| s.polish.map(|(chars, secs)| (chars.max(20) as f32, secs)));
        estimate(POLISH_RATE, chars as f32, samples)
    }

    pub fn record(&mut self, models: Models, audio_secs: f32, out: &Output) {
        if self.0.len() == LIMIT {
            self.0.pop_front();
        }
        self.0.push_back(Sample {
            models,
            audio_secs,
            asr_secs: out.asr_ms as f32 / 1000.,
            polish: out.polished.as_ref().map(|_| (out.raw.chars().count(), out.polish_ms as f32 / 1000.)),
        });
    }
}

fn estimate(default_rate: f32, work: f32, samples: impl Iterator<Item = (f32, f32)>) -> f32 {
    let (sum, weight) = samples.fold((default_rate * PRIOR_WEIGHT, PRIOR_WEIGHT), |(sum, weight), (units, secs)| {
        let rate = ((secs - OVERHEAD_S).max(0.) / units).clamp(default_rate * 0.1, default_rate * 20.);
        (sum + rate, weight + 1.)
    });
    OVERHEAD_S + work * sum / weight
}

#[cfg(test)]
mod tests {
    use crate::i18n::Lang;

    use super::{Models, Output, TimingHistory};

    #[test]
    fn uses_only_recent_successful_timings_for_the_matching_stage_model() {
        let mut history = TimingHistory::default();
        let models = Models { asr: 1, polish: 2 };
        let mut out = Output {
            raw: "word ".repeat(16),
            polished: Some("text".into()),
            polish_error: None,
            lang: Lang::En,
            asr_ms: 8600,
            polish_ms: 4600,
        };
        history.record(models, 10., &out);
        assert!(history.recognition(1, 10.) > history.recognition(3, 10.));
        assert!(history.polishing(2, 80) > history.polishing(4, 80));
        out.polished = None;
        for _ in 0..10 {
            history.record(Models { asr: 3, polish: 2 }, 10., &out);
        }
        assert_eq!(history.recognition(1, 10.), 3.1);
        assert_eq!(history.polishing(2, 80), 1.6);
    }
}
