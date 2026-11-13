use dito_core::config::{DEFAULT_ASR_LOCAL, DEFAULT_TEXT_LOCAL};
use dito_core::i18n::Tr;

pub(in crate::onboarding) struct Tier {
    pub(in crate::onboarding) name: Tr,
    pub(in crate::onboarding) desc: Tr,
    pub(in crate::onboarding) asr: &'static str,
    pub(in crate::onboarding) polish: &'static str,
}

/// The last one is the app default.
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
        asr: "qwen3-asr-0.6b",
        polish: "qwen3.5-4b-instruct",
    },
    Tier {
        name: Tr::OnboardingTierHigh,
        desc: Tr::OnboardingTierHighDesc,
        asr: DEFAULT_ASR_LOCAL,
        polish: DEFAULT_TEXT_LOCAL,
    },
];
