use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::i18n::Lang;
use crate::prompts::Prompts;

pub const DEFAULT_SHORTCUT: &str = "RightCommand";
pub const DEFAULT_ASR_LOCAL: &str = "qwen3-asr-1.7b";
pub const DEFAULT_POLISH_LOCAL: &str = "qwen3.5-9b-instruct";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Local,
    Provider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Azure,
    Bedrock,
    Compat,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 3] = [Self::Azure, Self::Bedrock, Self::Compat];

    pub fn label(self) -> crate::i18n::Tr {
        use crate::i18n::Tr;
        match self {
            Self::Azure => Tr::ModelsProviderAzure,
            Self::Bedrock => Tr::ModelsProviderBedrock,
            Self::Compat => Tr::ModelsOpenaiCompatible,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Azure => "azure",
            Self::Bedrock => "bedrock",
            Self::Compat => "compat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(from = "String", into = "String")]
pub enum ReasoningEffort {
    Custom(String),
    Minimal,
    #[default]
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub const ALL: [ReasoningEffort; 4] = [Self::Minimal, Self::Low, Self::Medium, Self::High];

    pub fn as_str(&self) -> &str {
        match self {
            Self::Custom(value) => value,
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl From<String> for ReasoningEffort {
    fn from(value: String) -> Self {
        match value.as_str() {
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            _ => Self::Custom(value),
        }
    }
}
impl From<ReasoningEffort> for String {
    fn from(value: ReasoningEffort) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AzureSettings {
    pub endpoint: String,
    pub deployment: String,
    pub reasoning_effort: ReasoningEffort,
}

impl Default for AzureSettings {
    fn default() -> Self {
        Self { endpoint: String::new(), deployment: String::new(), reasoning_effort: ReasoningEffort::Low }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BedrockSettings {
    pub profile: String,
    pub region: String,
    pub model_id: String,
    pub reasoning_effort: ReasoningEffort,
}

impl Default for BedrockSettings {
    fn default() -> Self {
        Self {
            profile: String::new(),
            region: "ap-northeast-1".into(),
            model_id: String::new(),
            reasoning_effort: ReasoningEffort::Custom(String::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompatSettings {
    pub base_url: String,
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
}

impl Default for CompatSettings {
    fn default() -> Self {
        Self { base_url: String::new(), model: String::new(), reasoning_effort: ReasoningEffort::Low }
    }
}

/// All three provider forms are kept so switching the tab never loses what was typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderSettings {
    pub kind: ProviderKind,
    pub azure: AzureSettings,
    pub bedrock: BedrockSettings,
    pub compat: CompatSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub cloud_asr: Option<crate::provider::configured_asr::AsrProfiles>,
    pub source: Source,
    pub local_model: String,
    pub provider: ProviderSettings,
}

impl EngineConfig {
    fn with_local(model: &str) -> Self {
        Self {
            source: Source::Local,
            local_model: model.into(),
            provider: ProviderSettings::default(),
            cloud_asr: None,
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::with_local(DEFAULT_ASR_LOCAL)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolishConfig {
    pub enabled: bool,
    #[serde(flatten)]
    pub engine: EngineConfig,
}

impl Default for PolishConfig {
    fn default() -> Self {
        Self { enabled: true, engine: EngineConfig::with_local(DEFAULT_POLISH_LOCAL) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordMode {
    #[default]
    Toggle,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MusicMuteMode {
    Off,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InsertMethod {
    /// Type the text as synthetic key events.
    #[default]
    KeyInput,
    /// Put the text on the clipboard and send Cmd+V.
    Paste,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// Language the speaker most likely uses; `Auto` lets the engine detect it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PreferredLanguage {
    #[default]
    Auto,
    Ja,
    En,
    Zh,
}

impl PreferredLanguage {
    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Ja => Some("ja"),
            Self::En => Some("en"),
            Self::Zh => Some("zh"),
        }
    }
}

/// Language of the app's own UI: follows the system unless the user picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayLanguage {
    #[default]
    System,
    Ja,
    En,
    Zh,
}

impl DisplayLanguage {
    pub const ALL: [DisplayLanguage; 4] = [Self::System, Self::Ja, Self::En, Self::Zh];

    pub fn resolve(self) -> Lang {
        match self {
            Self::System => crate::i18n::system_language(),
            Self::Ja => Lang::Ja,
            Self::En => Lang::En,
            Self::Zh => Lang::Zh,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub shortcut: String,
    pub record_mode: RecordMode,
    pub insert_method: InsertMethod,
    pub music_mute: MusicMuteMode,
    pub microphone: String,
    /// Per-device choice (by name) of whether automatic microphone picking may use it. Devices
    /// not listed follow the default for their connection type.
    pub microphone_auto: BTreeMap<String, bool>,
    pub preferred_language: PreferredLanguage,
    pub launch_at_login: bool,
    pub appearance: Appearance,
    /// HUD appearance follows the OS independently of the settings window.
    pub hud_appearance: Appearance,
    pub hud_position: HudPosition,
    /// Distance in points between the countdown's bottom and the usable screen edge.
    /// Retained for migration of settings written before discrete height levels existed.
    pub hud_offset: u32,
    /// Discrete position, -2 through 12. Missing in older configuration files.
    pub hud_height_level: Option<i8>,
    pub display_language: DisplayLanguage,
}

impl General {
    /// The language the UI is shown in right now.
    pub fn lang(&self) -> Lang {
        self.display_language.resolve()
    }

    pub fn hud_height_level(&self) -> i8 {
        self.hud_height_level.unwrap_or_else(|| ((self.hud_offset as f64 - 8.) / 19.).round() as i8).clamp(-2, 12)
    }

    /// Signed clearance of the countdown's bottom from the usable screen edge.
    pub fn hud_clearance(&self) -> i32 {
        8 + i32::from(self.hud_height_level()) * 19
    }
}

impl Default for General {
    fn default() -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.into(),
            record_mode: RecordMode::Toggle,
            insert_method: InsertMethod::KeyInput,
            music_mute: MusicMuteMode::Auto,
            microphone: String::new(),
            microphone_auto: BTreeMap::new(),
            preferred_language: PreferredLanguage::Auto,
            launch_at_login: true,
            appearance: Appearance::System,
            hud_appearance: Appearance::Dark,
            hud_position: HudPosition::Centered,
            hud_offset: 8,
            hud_height_level: None,
            display_language: DisplayLanguage::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudPosition {
    #[default]
    Centered,
    Pointer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub asr: EngineConfig,
    pub polish: PolishConfig,
    pub prompts: Prompts,
    /// Also pass dictionary words to the polish model.
    pub dictionary_for_polish: bool,
    pub onboarded: bool,
    pub models_dir: Option<PathBuf>,
}

impl Config {
    /// First-run defaults for development: Azure MAI-Transcribe-2 + gpt-6.1-terra (low).
    pub fn dev_azure() -> Self {
        let mut c = Self::default();
        c.asr.source = Source::Provider;
        c.asr.provider.kind = ProviderKind::Azure;
        c.asr.provider.azure.deployment = "MAI-Transcribe-2".into();
        c.polish.engine.source = Source::Provider;
        c.polish.engine.provider.kind = ProviderKind::Azure;
        c.polish.engine.provider.azure.deployment = "gpt-6.1-terra".into();
        c.polish.engine.provider.azure.reasoning_effort = ReasoningEffort::Low;
        c
    }

    /// Ids of the local models the current settings would run: speech recognition when it is local, and
    /// polishing when it is enabled and local.
    pub fn local_models_in_use(&self) -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        if self.asr.source == Source::Local {
            ids.insert(self.asr.local_model.clone());
        }
        if self.polish.enabled && self.polish.engine.source == Source::Local {
            ids.insert(self.polish.engine.local_model.clone());
        }
        ids
    }
}

pub fn default_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "fang2hou", "Dito")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".dito"))
}

/// Single source of truth for settings: persisted as TOML, observable through a watch channel.
pub struct ConfigStore {
    path: PathBuf,
    tx: watch::Sender<Arc<Config>>,
}

impl ConfigStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let cfg = match std::fs::read_to_string(&path) {
            Ok(text) => {
                toml::from_str::<Config>(&text).with_context(|| format!("invalid config: {}", path.display()))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        let (tx, _) = watch::channel(Arc::new(cfg));
        Ok(Self { path, tx })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self) -> Arc<Config> {
        self.tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<Config>> {
        self.tx.subscribe()
    }

    /// Apply `f`, persist atomically, then notify subscribers. Nothing is notified if the write fails.
    pub fn update(&self, f: impl FnOnce(&mut Config)) -> Result<()> {
        let mut next = (*self.get()).clone();
        f(&mut next);
        if next == *self.get() {
            return Ok(());
        }
        self.write(&next)?;
        self.tx.send_replace(Arc::new(next));
        Ok(())
    }

    fn write(&self, cfg: &Config) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(cfg)?;
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_enabled_engines_count_as_models_in_use() {
        let mut c = Config::default();
        assert_eq!(c.local_models_in_use().len(), 2, "default is local ASR + local polish");
        c.polish.enabled = false;
        assert_eq!(c.local_models_in_use(), BTreeSet::from([c.asr.local_model.clone()]));
        c.asr.source = Source::Provider;
        assert!(c.local_models_in_use().is_empty());
        c.polish.enabled = true;
        assert_eq!(c.local_models_in_use(), BTreeSet::from([c.polish.engine.local_model.clone()]));
    }

    #[test]
    fn missing_file_yields_defaults_and_update_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(*store.get(), Config::default());
        store.update(|c| c.general.launch_at_login = false).unwrap();
        let reopened = ConfigStore::open(&path).unwrap();
        assert!(!reopened.get().general.launch_at_login);
    }

    #[test]
    fn unknown_or_missing_fields_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nsound_effects = false\n").unwrap(); // removed field: ignored
        let cfg = ConfigStore::open(&path).unwrap().get();
        assert_eq!(cfg.general.shortcut, DEFAULT_SHORTCUT);
        assert_eq!(cfg.general.music_mute, MusicMuteMode::Auto, "legacy config without the field mutes by default");
        assert_eq!(cfg.general.hud_appearance, Appearance::Dark);
        assert_eq!(cfg.polish.engine.local_model, DEFAULT_POLISH_LOCAL);
    }

    #[test]
    fn subscribers_see_updates_but_noop_updates_are_silent() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::open(dir.path().join("c.toml")).unwrap();
        let mut rx = store.subscribe();
        store.update(|_| {}).unwrap();
        assert!(!rx.has_changed().unwrap());
        store.update(|c| c.onboarded = true).unwrap();
        assert!(rx.has_changed().unwrap());
        assert!(rx.borrow_and_update().onboarded);
    }

    #[test]
    fn hud_appearance_persists_independently_of_the_interface() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let store = ConfigStore::open(&path).unwrap();
        store
            .update(|c| {
                c.general.appearance = Appearance::Light;
                c.general.hud_appearance = Appearance::Dark;
            })
            .unwrap();
        let restored = ConfigStore::open(&path).unwrap();
        assert_eq!(restored.get().general.appearance, Appearance::Light);
        assert_eq!(restored.get().general.hud_appearance, Appearance::Dark);
        restored.update(|c| c.general.appearance = Appearance::System).unwrap();
        assert_eq!(restored.get().general.hud_appearance, Appearance::Dark);
    }

    #[test]
    fn hud_position_defaults_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let store = ConfigStore::open(path.clone()).unwrap();
        assert_eq!(store.get().general.hud_position, HudPosition::Centered);
        assert_eq!(store.get().general.hud_offset, 8);
        store
            .update(|c| {
                c.general.hud_position = HudPosition::Pointer;
                c.general.hud_offset = 0;
            })
            .unwrap();
        let restored = ConfigStore::open(path).unwrap();
        assert_eq!(restored.get().general.hud_position, HudPosition::Pointer);
        assert_eq!(restored.get().general.hud_offset, 0);
    }

    #[test]
    fn hud_levels_preserve_old_height_and_allow_negative_clearance() {
        let mut general = General::default();
        assert_eq!((general.hud_height_level(), general.hud_clearance()), (0, 8));
        general.hud_offset = 46;
        assert_eq!((general.hud_height_level(), general.hud_clearance()), (2, 46));
        general.hud_height_level = Some(-2);
        assert_eq!(general.hud_clearance(), -30);
        general.hud_height_level = Some(100);
        assert_eq!(general.hud_height_level(), 12);
    }

    #[test]
    fn invalid_toml_is_an_error_not_silent_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "general = [").unwrap();
        assert!(ConfigStore::open(&path).is_err());
    }

    #[test]
    fn dev_defaults_match_the_agreed_models() {
        let c = Config::dev_azure();
        assert_eq!(c.asr.provider.azure.deployment, "MAI-Transcribe-2");
        assert_eq!(c.polish.engine.provider.azure.deployment, "gpt-6.1-terra");
        assert_eq!(c.polish.engine.provider.azure.reasoning_effort, ReasoningEffort::Low);
    }
}
