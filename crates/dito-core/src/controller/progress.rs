//! In-memory timings keyed by the engines in use: estimates for the HUD and the most recent step times.

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
    pub transcription_secs: f32,
    /// The actual polishing start and its estimated duration, including time spent during the HUD catch-up.
    pub polishing: Option<(Instant, f32)>,
}

impl Default for ProcessingEstimate {
    fn default() -> Self {
        Self { transcription_secs: OVERHEAD_S + ASR_RATE, polishing: None }
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
                let configurations = engine.cloud_asr.clone().unwrap_or_default();
                let active = configurations.active().map(|c| (c.id.clone(), c.config.to_string()));
                ("configuration", active).hash(&mut key);
            } else {
                // A new model or other parameters start a new estimate; the name does not.
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

/// How long each step of the most recent dictation took. Kept per engine, so a newly selected model has no time
/// until it has run.
#[derive(Clone, Copy, Default)]
pub struct StepTimes {
    asr: Option<(u64, f32)>,
    polish: Option<(u64, f32)>,
}

impl StepTimes {
    pub(super) fn record(&mut self, models: Models, out: &Output) {
        self.asr = Some((models.asr, out.asr_ms as f32 / 1000.));
        if out.polished.is_some() {
            self.polish = Some((models.polish, out.polish_ms as f32 / 1000.));
        }
    }

    pub fn transcription_secs(&self, engine: &EngineConfig) -> Option<f32> {
        self.asr.filter(|(key, _)| *key == engine_key(engine, true)).map(|(_, secs)| secs)
    }

    pub fn polishing_secs(&self, engine: &EngineConfig) -> Option<f32> {
        self.polish.filter(|(key, _)| *key == engine_key(engine, false)).map(|(_, secs)| secs)
    }
}

#[derive(Default)]
pub(super) struct TimingHistory(VecDeque<Sample>);

impl TimingHistory {
    pub fn transcription(&self, model: u64, audio_secs: f32) -> f32 {
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

    use super::{EngineConfig, Models, Output, Source, StepTimes, TimingHistory};

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
        assert!(history.transcription(1, 10.) > history.transcription(3, 10.));
        assert!(history.polishing(2, 80) > history.polishing(4, 80));
        out.polished = None;
        for _ in 0..10 {
            history.record(Models { asr: 3, polish: 2 }, 10., &out);
        }
        assert_eq!(history.transcription(1, 10.), 3.1);
        assert_eq!(history.polishing(2, 80), 1.6);
    }

    #[test]
    fn step_times_belong_to_the_engine_that_ran_and_survive_a_failed_polish() {
        let engine = |model: &str| EngineConfig {
            cloud_asr: None,
            cloud_polish: None,
            source: Source::Local,
            local_model: model.into(),
            local_params: Default::default(),
        };
        let (asr, polish) = (engine("speech"), engine("writer"));
        let mut out = Output {
            raw: "raw".into(),
            polished: Some("text".into()),
            polish_error: None,
            lang: Lang::En,
            asr_ms: 400,
            polish_ms: 700,
        };
        let mut times = StepTimes::default();
        times.record(Models::new(&asr, &polish), &out);
        (out.polished, out.asr_ms, out.polish_ms) = (None, 900, 50);
        times.record(Models::new(&asr, &polish), &out);
        assert_eq!(times.transcription_secs(&asr), Some(0.9));
        assert_eq!(times.polishing_secs(&polish), Some(0.7));
        assert_eq!(times.transcription_secs(&engine("other")), None);
        assert_eq!(times.polishing_secs(&engine("other")), None);
    }
}
