use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::watch;

use super::migrate::{self, SCHEMA_VERSION, UpgradeError};
use super::{CloudAsrConfigurations, Config, ConfigError};
use crate::secrets::SecretStore;

type Result<T> = std::result::Result<T, ConfigError>;

pub struct ConfigStore {
    path: PathBuf,
    tx: watch::Sender<Arc<Config>>,
}

impl ConfigStore {
    /// A missing file yields the defaults and writes nothing. An older file is upgraded first, its original kept as
    /// `<file>.v<old version>.bak`. A newer file is refused and left untouched.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let (cfg, found) = match std::fs::read_to_string(&path) {
            Ok(text) => load(&path, &text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), SCHEMA_VERSION),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        let (tx, _) = watch::channel(Arc::new(cfg));
        let store = Self { path, tx };
        if found < SCHEMA_VERSION {
            back_up(&store.path, found)?;
            store.write(&store.get())?;
        }
        Ok(store)
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

    /// The list is saved first, so a failed save keeps the configuration and its key together. The last configuration
    /// and its key stay: `false`.
    pub fn remove_cloud_polish_configuration(&self, id: &str, secrets: &dyn SecretStore) -> Result<bool> {
        let mut removed = None;
        self.update(|c| removed = c.polish.engine.polish_configurations_mut(c.general.lang()).remove(id))?;
        Ok(forget_key(removed.map(|configuration| configuration.account()), secrets))
    }

    pub fn remove_cloud_asr_configuration(&self, id: &str, secrets: &dyn SecretStore) -> Result<bool> {
        let mut removed = None;
        self.update(|c| removed = c.asr.cloud_asr.get_or_insert_with(CloudAsrConfigurations::default).remove(id))?;
        Ok(forget_key(removed.map(|configuration| configuration.account()), secrets))
    }

    fn write(&self, cfg: &Config) -> Result<()> {
        write_file(&self.path, cfg)
    }
}

/// Delete the key of an entry that was removed, and say whether there was one. A key that cannot be deleted stays in
/// the Keychain under an id that is never used again: that is logged, because the entry is gone either way.
fn forget_key(account: Option<String>, secrets: &dyn SecretStore) -> bool {
    let Some(account) = account else { return false };
    if let Err(error) = secrets.delete(&account) {
        tracing::warn!("could not delete the API key of a removed entry: {error}");
    }
    true
}

/// Write `cfg` to `path` through a temporary file beside it, so a crash leaves the old file or the new one, never a
/// half-written one.
pub(super) fn write_file(path: &Path, cfg: &Config) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string_pretty(cfg)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn load(path: &Path, text: &str) -> Result<(Config, u32)> {
    let invalid = |source| ConfigError::Invalid { path: path.to_owned(), source };
    let mut table = text.parse::<toml::Table>().map_err(invalid)?;
    let found = migrate::upgrade(&mut table).map_err(|e| match e {
        UpgradeError::Malformed(source) => invalid(source),
        UpgradeError::Newer(found) => {
            ConfigError::NewerSchema { path: path.to_owned(), found, supported: SCHEMA_VERSION }
        }
    })?;
    let cfg: Config = table.try_into().map_err(invalid)?;
    Ok((cfg, found))
}

/// Keep the file as it was before an upgrade, as `<file>.v<version>.bak` beside it. A backup that already exists is the
/// older original and stays.
fn back_up(path: &Path, version: u32) -> std::io::Result<()> {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".v{version}.bak"));
    let backup = PathBuf::from(name);
    if backup.try_exists()? {
        return Ok(());
    }
    // Copy under a temporary name first: a copy that fails midway must not leave a truncated file that a later run
    // would trust as the backup.
    let partial = backup.with_extension("bak.tmp");
    std::fs::copy(path, &partial)?;
    std::fs::rename(&partial, &backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Appearance, CloudAsrConfigurations, DEFAULT_POLISH_LOCAL, DEFAULT_SHORTCUT, MusicMuteMode, ReasoningEffort,
    };
    use crate::i18n::Lang;
    use crate::prompts::{Style, default_set};

    fn config_file(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    fn file_count(dir: &tempfile::TempDir) -> usize {
        std::fs::read_dir(dir.path()).unwrap().count()
    }

    #[test]
    fn missing_file_yields_defaults_and_update_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(*store.get(), Config::default());
        assert!(!path.exists(), "opening alone writes nothing");
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
    fn invalid_toml_is_an_error_not_silent_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "general = [").unwrap();
        assert!(matches!(ConfigStore::open(&path), Err(ConfigError::Invalid { .. })));
    }

    #[test]
    fn cloud_speech_nulls_and_custom_requests_survive_toml_storage() {
        let mut config = Config::default();
        let mut configurations = CloudAsrConfigurations::default();
        configurations.select("openrouter");
        configurations.items[2].config["connection"]["model"] = "my-model".into();
        configurations.items[2].config["language_map"]["ja"] = "JP".into();
        config.asr.cloud_asr = Some(configurations);
        let encoded = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(config, decoded);
        assert!(decoded.asr.cloud_asr.unwrap().active().unwrap().config["language_map"]["auto"].is_null());
    }

    #[test]
    fn reasoning_effort_accepts_future_values_and_empty_default() {
        for value in ["low", "", "xhigh", "provider-new-value"] {
            let effort: ReasoningEffort = value.to_owned().into();
            assert_eq!(effort.as_str(), value);
            assert_eq!(serde_json::to_string(&effort).unwrap(), serde_json::to_string(value).unwrap());
        }
    }

    #[test]
    fn legacy_file_is_upgraded_once_and_the_original_is_kept() {
        let original = "onboarded = true\n\n[general]\nshortcut = \"LeftOption\"\n";
        let (dir, path) = config_file(original);

        let cfg = ConfigStore::open(&path).unwrap().get();
        assert!(cfg.onboarded);
        assert_eq!(cfg.general.shortcut, "LeftOption");
        assert_eq!(cfg.schema_version, SCHEMA_VERSION);
        assert_eq!(std::fs::read(dir.path().join("config.toml.v0.bak")).unwrap(), original.as_bytes());
        let upgraded = std::fs::read_to_string(&path).unwrap();
        assert!(upgraded.starts_with(&format!("schema_version = {SCHEMA_VERSION}\n")), "{upgraded}");

        assert_eq!(ConfigStore::open(&path).unwrap().get(), cfg);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), upgraded);
        assert_eq!(file_count(&dir), 2);
    }

    #[test]
    fn an_existing_backup_is_never_replaced() {
        let (dir, path) = config_file("onboarded = true\n");
        let backup = dir.path().join("config.toml.v0.bak");
        std::fs::write(&backup, "the first original").unwrap();

        assert!(ConfigStore::open(&path).unwrap().get().onboarded);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "the first original");
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("schema_version"), "the file is still upgraded");
    }

    #[test]
    fn legacy_prompts_get_the_default_herdr_style_unless_the_user_wrote_one() {
        let (_dir, path) = config_file(
            r#"
[prompts.ja]
base = "my existing rule"

[prompts.en.herdr]
instruction = "my herdr rule"
"#,
        );

        let prompts = ConfigStore::open(path).unwrap().get().prompts.clone();
        let herdr = Style::builtin("herdr").unwrap();
        assert_eq!(prompts.ja.base, "my existing rule");
        assert_eq!(prompts.ja.style(&herdr), default_set(Lang::Ja).style(&herdr));
        assert_eq!(prompts.en.style(&herdr).instruction, "my herdr rule");
        assert_eq!(prompts.zh.style(&herdr), default_set(Lang::Zh).style(&herdr));
    }

    #[test]
    fn a_version_one_cloud_speech_list_keeps_its_selection_edits_and_keychain_account() {
        let edited = r#"{"connection":{"model":"my-model"},"language_map":{"auto":null,"ja":"JP"}}"#;
        let untouched = "{}";
        let (dir, path) = config_file(&format!(
            r#"
schema_version = 1

[asr]
source = "provider"

[asr.cloud_asr]
selected = 2
profiles = ['{untouched}', '{untouched}', '{edited}']
"#
        ));

        let cfg = ConfigStore::open(&path).unwrap().get();
        let configurations = cfg.asr.cloud_asr.as_ref().unwrap();
        assert_eq!(configurations.selected, "openrouter");
        let active = configurations.active().unwrap();
        assert_eq!(active.id, "openrouter");
        assert_eq!(active.account(), "asr.openrouter", "the Keychain account is the one the position had");
        assert_eq!(configurations.active_name(), Some("OpenRouter"));
        let expected: serde_json::Value = serde_json::from_str(edited).unwrap();
        assert_eq!(active.config, expected, "edits and nulls stay");
        assert!(dir.path().join("config.toml.v1.bak").exists(), "the original is kept");

        assert_eq!(ConfigStore::open(&path).unwrap().get(), cfg);
    }

    #[test]
    fn a_version_two_file_keeps_its_polish_behavior_as_configurations_and_the_original_is_kept() {
        let original = r#"schema_version = 2
onboarded = true

[general]
display_language = "ja"

[asr]
source = "local"
local_model = "qwen3-asr-1.7b"

[asr.provider]
kind = "azure"

[asr.provider.azure]
endpoint = ""
deployment = ""
reasoning_effort = "low"

[polish]
enabled = true
source = "provider"
local_model = "qwen3.5-9b-instruct"

[polish.provider]
kind = "compat"

[polish.provider.azure]
endpoint = "https://dito.openai.azure.com/"
deployment = "gpt-6.1-terra"
reasoning_effort = "low"

[polish.provider.bedrock]
profile = "work"
region = "ap-northeast-1"
model_id = "jp.anthropic.claude-sonnet-4-5"
reasoning_effort = "low"

[polish.provider.compat]
base_url = "https://router.example/v1"
model = "vendor/model"
reasoning_effort = "minimal"
"#;
        let (dir, path) = config_file(original);

        let cfg = ConfigStore::open(&path).unwrap().get();
        let configurations = cfg.polish.engine.polish_configurations(cfg.general.lang());
        assert_eq!(configurations.selected, "compat", "the selected form stays selected");
        let compat = configurations.active().unwrap();
        assert_eq!(
            (compat.name.as_str(), compat.target.address(), compat.target.model()),
            ("OpenAI 互換 API", "https://router.example/v1", "vendor/model")
        );
        assert_eq!(compat.params.reasoning_effort.as_str(), "minimal");
        assert_eq!(compat.account(), "polish.compat", "the key saved under the old account keeps working");
        assert_eq!(cfg.polish.engine.label(), "compat:vendor/model");
        let bedrock = configurations.get("bedrock").unwrap();
        assert_eq!((bedrock.params.temperature, bedrock.params.max_tokens), (Some(0.2), Some(2048)));
        assert_eq!(bedrock.params.reasoning_effort.as_str(), "low");
        assert_eq!(configurations.get("azure").unwrap().account(), "polish.azure");
        assert!(cfg.onboarded && cfg.asr.cloud_asr.is_none(), "the rest of the file is as it was");

        assert_eq!(std::fs::read(dir.path().join("config.toml.v2.bak")).unwrap(), original.as_bytes());
        let upgraded = std::fs::read_to_string(&path).unwrap();
        assert!(upgraded.starts_with(&format!("schema_version = {}\n", crate::config::SCHEMA_VERSION)), "{upgraded}");
        assert!(upgraded.contains("[polish.cloud_polish]"), "{upgraded}");
        assert!(!upgraded.contains("[polish.provider") && !upgraded.contains("[asr.provider"), "{upgraded}");

        assert_eq!(ConfigStore::open(&path).unwrap().get(), cfg);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), upgraded);
        assert_eq!(file_count(&dir), 2);
    }

    #[test]
    fn configurations_and_local_parameters_survive_toml_and_the_speech_section_stays_free_of_them() {
        use crate::config::{CloudPolishConfigurations, CloudPolishTemplate, LocalPolishParams};
        let mut config = Config::default();
        let mut configurations = CloudPolishConfigurations::new(Lang::En);
        let added = configurations.add_from_template(CloudPolishTemplate::Azure, "Mine");
        let mine = configurations.get_mut(&added).unwrap();
        *mine.target.model_mut() = "gpt-x".into();
        mine.params.temperature = Some(0.7);
        mine.params.max_tokens = Some(300);
        mine.params.extra = serde_json::from_str(r#"{"seed": 7, "stop": null}"#).unwrap();
        config.polish.engine.cloud_polish = Some(configurations);
        let params =
            LocalPolishParams { temperature: Some(0.3), top_p: Some(0.8), top_k: Some(40), max_tokens: Some(512) };
        config.polish.engine.set_local_params("qwen3.5-9b-instruct", params);
        config.polish.engine.set_local_params("qwen3.5-2b-instruct", LocalPolishParams::default());

        let encoded = toml::to_string(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&encoded).unwrap(), config);
        let table: toml::Table = encoded.parse().unwrap();
        assert!(table["asr"].get("cloud_polish").is_none() && table["asr"].get("local_params").is_none());
        assert!(table["polish"].get("cloud_asr").is_none());
        assert_eq!(
            table["polish"]["local_params"].as_table().unwrap().len(),
            1,
            "parameters that set nothing are not kept"
        );
        assert_eq!(config.polish.engine.local_params_for("qwen3.5-9b-instruct"), params);
        assert_eq!(config.polish.engine.local_params_for("other"), LocalPolishParams::default());
    }

    #[test]
    fn removing_a_polish_or_speech_configuration_saves_the_list_and_deletes_its_key() {
        use crate::secrets::{MemorySecrets, SecretError};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let store = ConfigStore::open(&path).unwrap();
        let secrets = MemorySecrets::default();
        for account in ["polish.azure", "polish.bedrock", "polish.compat", "asr.funasr", "asr.openrouter"] {
            secrets.set(account, "key").unwrap();
        }

        assert!(store.remove_cloud_polish_configuration("azure", &secrets).unwrap());
        let config = store.get();
        let list = config.polish.engine.polish_configurations(config.general.lang());
        assert_eq!(list.items.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["bedrock", "compat"]);
        assert_eq!(list.selected, "bedrock");
        assert_eq!(secrets.get("polish.azure"), None);
        assert_eq!(secrets.get("polish.compat").as_deref(), Some("key"));
        let reopened = ConfigStore::open(&path).unwrap().get();
        assert_eq!(reopened.polish.engine.polish_configurations(reopened.general.lang()).items.len(), 2, "on disk");

        assert!(store.remove_cloud_polish_configuration("bedrock", &secrets).unwrap());
        assert!(!store.remove_cloud_polish_configuration("compat", &secrets).unwrap());
        assert!(!store.remove_cloud_polish_configuration("missing", &secrets).unwrap());
        assert_eq!(secrets.get("polish.compat").as_deref(), Some("key"));

        assert!(store.remove_cloud_asr_configuration("funasr", &secrets).unwrap());
        assert_eq!(secrets.get("asr.funasr"), None);
        assert_eq!(secrets.get("asr.openrouter").as_deref(), Some("key"));
        assert_eq!(store.get().asr.cloud_asr.as_ref().unwrap().selected, "qwen_asr");

        // A Keychain that refuses the deletion does not bring the configuration back.
        struct Locked;
        impl SecretStore for Locked {
            fn get(&self, _: &str) -> Option<String> {
                None
            }
            fn set(&self, _: &str, _: &str) -> std::result::Result<(), SecretError> {
                Err(SecretError::Backend("locked".into()))
            }
            fn delete(&self, _: &str) -> std::result::Result<(), SecretError> {
                Err(SecretError::Backend("locked".into()))
            }
        }
        assert!(store.remove_cloud_asr_configuration("qwen_asr", &Locked).unwrap());
        assert_eq!(store.get().asr.cloud_asr.as_ref().unwrap().items.len(), 1);
    }

    #[test]
    fn a_newer_schema_is_refused_and_the_file_is_left_alone() {
        let newer = "schema_version = 999\nfuture_setting = true\n";
        let (dir, path) = config_file(newer);

        let Err(refused) = ConfigStore::open(&path) else { panic!("a newer schema must not load") };
        assert!(matches!(refused, ConfigError::NewerSchema { found: 999, supported: SCHEMA_VERSION, .. }));
        let message = refused.to_string();
        assert!(message.contains("config.toml") && message.contains("999"), "{message}");
        assert_eq!(std::fs::read(&path).unwrap(), newer.as_bytes());
        assert_eq!(file_count(&dir), 1, "no backup or temporary file");
    }

    #[test]
    fn a_current_file_is_not_rewritten_on_open() {
        // A comment and hand-written layout: saving through the serializer would change these bytes.
        let current = format!("# my notes\nschema_version = {SCHEMA_VERSION}\nonboarded = true\n");
        let (dir, path) = config_file(&current);

        assert!(ConfigStore::open(&path).unwrap().get().onboarded);
        assert_eq!(std::fs::read(&path).unwrap(), current.as_bytes());
        assert_eq!(file_count(&dir), 1, "no backup");
    }

    #[test]
    fn schema_version_leads_the_serialized_config_and_round_trips() {
        let encoded = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(encoded.starts_with(&format!("schema_version = {SCHEMA_VERSION}\n")), "{encoded}");
        assert_eq!(toml::from_str::<Config>(&encoded).unwrap(), Config::default());
    }
}
