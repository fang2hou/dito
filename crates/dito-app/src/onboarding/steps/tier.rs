use dito_core::config::{DEFAULT_ASR_LOCAL, DEFAULT_TEXT_LOCAL};
use dito_core::i18n::Tr;

pub(in crate::onboarding) struct Tier {
    pub(in crate::onboarding) name: Tr,
    pub(in crate::onboarding) desc: Tr,
    pub(in crate::onboarding) asr: &'static str,
    pub(in crate::onboarding) polish: &'static str,
}

/// The balanced tier is the app default.
pub(in crate::onboarding) const TIERS: [Tier; 3] = [
    Tier {
        name: Tr::OnboardingTierLight,
        desc: Tr::OnboardingTierLightDesc,
        asr: "sensevoice-small",
        polish: "qwen3.5-2b-instruct",
    },
    Tier {
        name: Tr::OnboardingTierStandard,
        desc: Tr::OnboardingTierStandardDesc,
        asr: DEFAULT_ASR_LOCAL,
        polish: DEFAULT_TEXT_LOCAL,
    },
    Tier {
        name: Tr::OnboardingTierHigh,
        desc: Tr::OnboardingTierHighDesc,
        asr: "qwen3-asr-1.7b",
        polish: "qwen3.8-27b-instruct",
    },
];

#[cfg(test)]
mod tests {
    use dito_core::config::Config;

    use super::TIERS;

    #[test]
    fn a_new_config_selects_the_balanced_tier() {
        let config = Config::default();
        assert_eq!(config.asr.local_model, TIERS[1].asr);
        assert_eq!(config.text_model.local_model, TIERS[1].polish);
    }
}
