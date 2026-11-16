use serde::Serialize;

/// A model's wire value and its human-readable label. Callers send `key`, never `name`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThinkingLevel {
    pub key: &'static str,
    pub name: &'static str,
}

impl ThinkingLevel {
    pub const LOW: Self = Self { key: "low", name: "Low" };
    pub const MEDIUM: Self = Self { key: "medium", name: "Medium" };
    pub const HIGH: Self = Self { key: "high", name: "High" };
    pub const XHIGH: Self = Self { key: "xhigh", name: "Extra High" };
    pub const MAX: Self = Self { key: "max", name: "Max" };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThinkingCapabilities {
    pub can_disable: bool,
    /// Empty for a model with an on/off switch but no named effort levels.
    pub levels: &'static [ThinkingLevel],
    pub default_level: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TextModelCapabilities {
    /// Native input plus generated tokens, including thinking. This is not a memory allocation target.
    pub context_window_tokens: u32,
    /// A separate published output ceiling. `None` means output shares the remaining context budget.
    pub max_output_tokens: Option<u32>,
    /// Publisher guidance, not a hard limit or the product's default allocation.
    pub recommended_output_tokens: Option<u32>,
    pub thinking: Option<ThinkingCapabilities>,
}
