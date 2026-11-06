use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::watch;

use super::{Config, ConfigError};

type Result<T> = std::result::Result<T, ConfigError>;

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
                toml::from_str::<Config>(&text).map_err(|source| ConfigError::Invalid { path: path.clone(), source })?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(source) => return Err(ConfigError::Read { path, source }),
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
    use crate::config::{
        Appearance, AsrProfiles, DEFAULT_POLISH_LOCAL, DEFAULT_SHORTCUT, MusicMuteMode, ReasoningEffort,
    };

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
    fn invalid_toml_is_an_error_not_silent_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "general = [").unwrap();
        assert!(matches!(ConfigStore::open(&path), Err(ConfigError::Invalid { .. })));
    }

    #[test]
    fn profile_nulls_and_custom_requests_survive_toml_storage() {
        let mut config = Config::default();
        let mut profiles = AsrProfiles { selected: 2, ..Default::default() };
        profiles.profiles[2]["connection"]["model"] = "my-model".into();
        profiles.profiles[2]["language_map"]["ja"] = "JP".into();
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
}
