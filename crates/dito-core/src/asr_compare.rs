//! Comparing speech models on the user's own voice (`design:settings/compare-models`): which models take part,
//! the engine each one runs on, the blind order of a round and the ranking the ratings add up to.

use serde_json::Value;

use crate::config::{CloudAsrConfigurations, EngineConfig, Source};
use crate::provider::configured_asr::validate;

pub const MAX_CONTENDERS: usize = 5;
pub const MIN_CONTENDERS: usize = 2;

/// A downloaded local model or a saved cloud configuration, by id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Contender {
    Local(String),
    Cloud(String),
}

impl Contender {
    /// The one speech recognition uses now.
    pub fn in_use(engine: &EngineConfig) -> Option<Self> {
        match engine.source {
            Source::Local => Some(Self::Local(engine.local_model.clone())),
            Source::Provider => {
                engine.cloud_asr.as_ref().and_then(|list| list.active()).map(|c| Self::Cloud(c.id.clone()))
            }
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// The speech settings that run this contender; everything else stays as `base` has it.
    pub fn engine(&self, base: &EngineConfig) -> EngineConfig {
        let mut engine = base.clone();
        match self {
            Self::Local(id) => {
                engine.source = Source::Local;
                engine.local_model = id.clone();
            }
            Self::Cloud(id) => {
                engine.source = Source::Provider;
                engine.cloud_asr.get_or_insert_with(CloudAsrConfigurations::default).select(id);
            }
        }
        engine
    }
}

/// The choice a comparison opens with: the one in use, then the others in the order given, up to the limit.
pub fn initial_choice(available: &[Contender], in_use: Option<&Contender>) -> Vec<Contender> {
    let first = in_use.filter(|current| available.contains(current));
    first
        .into_iter()
        .chain(available.iter().filter(|contender| Some(*contender) != first))
        .take(MAX_CONTENDERS)
        .cloned()
        .collect()
}

/// Whether a cloud configuration can take part, and if not, what is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    /// Its request uses the API key and none is stored.
    MissingKey,
    /// The configuration does not validate, such as a template whose model ID is still empty.
    Incomplete,
}

pub fn cloud_readiness(config: &Value, has_key: bool) -> Readiness {
    if validate(config).is_err() {
        return Readiness::Incomplete;
    }
    if !has_key && config["request"].to_string().contains("{{api_key}}") {
        return Readiness::MissingKey;
    }
    Readiness::Ready
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rating {
    Good,
    Poor,
}

/// One contender's result in a finished round. `millis` is `None` when it failed: a failure is left out of the round,
/// so it carries no time and no rating.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub contender: Contender,
    pub millis: Option<u64>,
    pub rating: Option<Rating>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub good: u32,
    pub poor: u32,
    total_ms: u64,
    timed: u32,
}

impl Tally {
    pub fn score(&self) -> i64 {
        i64::from(self.good) - i64::from(self.poor)
    }

    pub fn average_ms(&self) -> Option<u64> {
        (self.timed > 0).then(|| self.total_ms / u64::from(self.timed))
    }
}

/// The tallies of every round so far, in the order the contenders were chosen.
#[derive(Clone, Debug, Default)]
pub struct Scoreboard {
    tallies: Vec<(Contender, Tally)>,
    rounds: u32,
}

impl Scoreboard {
    pub fn new(contenders: &[Contender]) -> Self {
        Self { tallies: contenders.iter().map(|c| (c.clone(), Tally::default())).collect(), rounds: 0 }
    }

    pub fn rounds(&self) -> u32 {
        self.rounds
    }

    pub fn has_ratings(&self) -> bool {
        self.tallies.iter().any(|(_, tally)| tally.good + tally.poor > 0)
    }

    pub fn add_round(&mut self, outcomes: &[Outcome]) {
        for outcome in outcomes {
            let Some(millis) = outcome.millis else { continue };
            let Some((_, tally)) = self.tallies.iter_mut().find(|(c, _)| *c == outcome.contender) else { continue };
            tally.total_ms += millis;
            tally.timed += 1;
            match outcome.rating {
                Some(Rating::Good) => tally.good += 1,
                Some(Rating::Poor) => tally.poor += 1,
                None => {}
            }
        }
        self.rounds += 1;
    }

    /// Highest score first; a tie goes to the faster average, and a contender that never answered comes after the
    /// timed ones it ties with.
    pub fn ranking(&self) -> Vec<(Contender, Tally)> {
        let mut ranked = self.tallies.clone();
        ranked.sort_by(|(_, a), (_, b)| {
            b.score().cmp(&a.score()).then_with(|| match (a.average_ms(), b.average_ms()) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
        });
        ranked
    }
}

/// The order a round shows its `n` results in, shuffled from `seed` so the labels say nothing about the model.
pub fn blind_order(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut state = seed;
    for i in (1..n).rev() {
        state = splitmix64(state);
        order.swap(i, (state % (i as u64 + 1)) as usize);
    }
    order
}

fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The letter a result shows in place of its model's name.
pub fn label(position: usize) -> char {
    char::from(b'A' + (position % 26) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(id: &str) -> Contender {
        Contender::Local(id.into())
    }

    fn cloud(id: &str) -> Contender {
        Contender::Cloud(id.into())
    }

    fn outcome(contender: Contender, millis: Option<u64>, rating: Option<Rating>) -> Outcome {
        Outcome { contender, millis, rating }
    }

    #[test]
    fn the_choice_starts_with_the_one_in_use_and_stops_at_five() {
        let available: Vec<_> = ["a", "b", "c", "d"].map(local).into_iter().chain(["x", "y"].map(cloud)).collect();
        let chosen = initial_choice(&available, Some(&cloud("y")));
        assert_eq!(chosen, vec![cloud("y"), local("a"), local("b"), local("c"), local("d")]);
        assert_eq!(initial_choice(&available, Some(&local("gone"))).len(), MAX_CONTENDERS);
        assert_eq!(initial_choice(&available[..2], None), vec![local("a"), local("b")]);
    }

    #[test]
    fn score_decides_then_speed_and_a_contender_that_never_answered_comes_last() {
        let (a, b, c, d) = (local("a"), local("b"), cloud("c"), cloud("d"));
        let mut board = Scoreboard::new(&[a.clone(), b.clone(), c.clone(), d.clone()]);
        board.add_round(&[
            outcome(a.clone(), Some(900), Some(Rating::Good)),
            outcome(b.clone(), Some(300), Some(Rating::Good)),
            outcome(c.clone(), Some(100), Some(Rating::Poor)),
            outcome(d.clone(), None, Some(Rating::Good)),
        ]);
        board.add_round(&[
            outcome(a.clone(), Some(1100), None),
            outcome(b.clone(), Some(500), None),
            outcome(c.clone(), Some(100), Some(Rating::Good)),
            outcome(d.clone(), None, None),
        ]);
        let ranking = board.ranking();
        let order: Vec<_> = ranking.iter().map(|(c, _)| c.clone()).collect();
        assert_eq!(order, vec![b.clone(), a.clone(), c.clone(), d.clone()]);
        assert_eq!(ranking[0].1.average_ms(), Some(400));
        assert_eq!((ranking[2].1.good, ranking[2].1.poor, ranking[2].1.score()), (1, 1, 0));
        assert_eq!(ranking[3].1, Tally::default(), "a failed result counts neither its time nor its rating");
        assert_eq!(board.rounds(), 2);
        assert!(board.has_ratings());
    }

    #[test]
    fn a_contender_runs_on_its_own_engine_and_keeps_the_rest_of_the_settings() {
        let mut base = EngineConfig { source: Source::Local, local_model: "in-use".into(), ..Default::default() };
        base.local_params.insert("in-use".into(), Default::default());
        let as_local = local("other").engine(&base);
        assert_eq!((as_local.source, as_local.local_model.as_str()), (Source::Local, "other"));
        assert_eq!(as_local.local_params, base.local_params);

        let template = CloudAsrConfigurations::default().items[1].id.clone();
        let as_cloud = cloud(&template).engine(&base);
        assert_eq!(as_cloud.source, Source::Provider);
        assert_eq!(as_cloud.cloud_asr.as_ref().and_then(|list| list.active()).map(|c| c.id.clone()), Some(template));
        assert_eq!(Contender::in_use(&as_cloud), Some(cloud(&as_cloud.cloud_asr.unwrap().selected)));
        assert_eq!(Contender::in_use(&base), Some(local("in-use")));
    }

    #[test]
    fn a_cloud_configuration_needs_a_valid_config_and_the_key_it_uses() {
        let template = CloudAsrConfigurations::default();
        let mut config = template.items.iter().find(|c| c.config["connection"]["model"] == "").unwrap().config.clone();
        assert_eq!(cloud_readiness(&config, true), Readiness::Incomplete, "a template with no model ID");
        config["connection"]["model"] = "asr-model".into();
        assert_eq!(cloud_readiness(&config, false), Readiness::MissingKey);
        assert_eq!(cloud_readiness(&config, true), Readiness::Ready);
        let keyless = config.to_string().replace("Bearer {{api_key}}", "Bearer none");
        assert_eq!(cloud_readiness(&serde_json::from_str(&keyless).unwrap(), false), Readiness::Ready);
    }

    #[test]
    fn the_blind_order_is_a_shuffle_of_every_result() {
        for seed in 0..50 {
            let mut order = blind_order(5, seed);
            order.sort_unstable();
            assert_eq!(order, vec![0, 1, 2, 3, 4]);
        }
        assert!((0..50).any(|seed| blind_order(5, seed) != vec![0, 1, 2, 3, 4]));
        assert_eq!((label(0), label(4)), ('A', 'E'));
    }
}
