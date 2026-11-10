use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::watch;

use super::migrate::{self, SCHEMA_VERSION, UpgradeError};
use super::{Config, ConfigError};

type Result<T> = std::result::Result<T, ConfigError>;

/// Single source of truth for settings: persisted as TOML, observable through a watch channel.
pub struct ConfigStore {
    path: PathBuf,
    tx: watch::Sender<Arc<Config>>,
}

impl ConfigStore {
    /// Load the settings at `path`. A missing file yields the defaults and writes nothing. A file saved by an older
    /// version is upgraded first: its original is kept beside it as `<file>.v<old version>.bak`, then the upgraded
    /// settings replace it. A file saved by a newer version is refused and left untouched.
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

/// Parse `text`, upgrade it to the current schema and return the settings together with the version the file was at.
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
        Appearance, AsrProfiles, DEFAULT_POLISH_LOCAL, DEFAULT_SHORTCUT, MusicMuteMode, ReasoningEffort,
    };
    use crate::i18n::Lang;
    use crate::prompts::default_set;

    /// A fresh directory holding `config.toml` with `contents`.
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
    fn profile_nulls_and_custom_requests_survive_toml_storage() {
        let mut config = Config::default();
        let mut profiles = AsrProfiles::default();
        profiles.select_index(2);
        profiles.profiles[2].config["connection"]["model"] = "my-model".into();
        profiles.profiles[2].config["language_map"]["ja"] = "JP".into();
        config.asr.cloud_asr = Some(profiles);
        let encoded = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(config, decoded);
        assert!(decoded.asr.cloud_asr.unwrap().active()["language_map"]["auto"].is_null());
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

        // The upgraded file is current: opening it again changes nothing and adds no backup.
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
        assert_eq!(prompts.ja.base, "my existing rule");
        assert_eq!(prompts.ja.herdr, default_set(Lang::Ja).herdr);
        assert_eq!(prompts.en.herdr.instruction, "my herdr rule");
        assert_eq!(prompts.zh.herdr, default_set(Lang::Zh).herdr);
    }

    #[test]
    fn legacy_cloud_asr_profiles_keep_their_selection_edits_and_keychain_account() {
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
        let profiles = cfg.asr.cloud_asr.as_ref().unwrap();
        assert_eq!(profiles.selected, "openrouter");
        assert_eq!(profiles.selected_index(), 2);
        assert_eq!(profiles.account(), "asr.openrouter", "the Keychain account is the one the position had");
        assert_eq!(profiles.active_name(), Some("OpenRouter"));
        let expected: serde_json::Value = serde_json::from_str(edited).unwrap();
        assert_eq!(profiles.active(), &expected, "edits and nulls stay");
        assert!(dir.path().join("config.toml.v1.bak").exists(), "the original is kept");

        // The rewritten file is in the new shape and loads back unchanged.
        assert_eq!(ConfigStore::open(&path).unwrap().get(), cfg);
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
