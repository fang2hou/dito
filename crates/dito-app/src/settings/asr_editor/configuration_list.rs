//! Changes to the saved cloud speech configurations. Each names its configuration by id, never by place: tiles are
//! reordered and the selection moves, and a draft that is still being checked must land in the configuration it was
//! typed into.

use std::borrow::Cow;

use anyhow::Result;
use dito_core::config::{CloudAsrConfigurations, CloudAsrTemplate, Config, ConfigStore};
use dito_core::secrets::SecretStore;
use serde_json::Value;

/// The saved list, or the bundled templates before anything was saved and after the list was emptied by hand.
pub(super) fn of(config: &Config) -> Cow<'_, CloudAsrConfigurations> {
    match &config.asr.cloud_asr {
        Some(list) if !list.items.is_empty() => Cow::Borrowed(list),
        _ => Cow::Owned(CloudAsrConfigurations::default()),
    }
}

/// The first change saves the bundled templates that stood in for the list.
fn of_mut(config: &mut Config) -> &mut CloudAsrConfigurations {
    let list = config.asr.cloud_asr.get_or_insert_with(CloudAsrConfigurations::default);
    if list.items.is_empty() {
        *list = CloudAsrConfigurations::default();
    }
    list
}

pub(super) fn select(cfg: &ConfigStore, id: &str) -> Result<()> {
    cfg.update(|config| {
        of_mut(config).select(id);
    })?;
    Ok(())
}

/// A blank name is refused and the last one stays.
pub(super) fn rename(cfg: &ConfigStore, id: &str, name: &str) -> Result<()> {
    cfg.update(|config| {
        of_mut(config).rename(id, name);
    })?;
    Ok(())
}

/// Put `id` at `index` of the list as it is afterwards.
pub(super) fn move_to(cfg: &ConfigStore, id: &str, index: usize) -> Result<()> {
    cfg.update(|config| {
        of_mut(config).move_to(id, index);
    })?;
    Ok(())
}

/// Also selects the new configuration.
pub(super) fn add(cfg: &ConfigStore, template: CloudAsrTemplate, name: &str) -> Result<String> {
    let mut added = String::new();
    cfg.update(|config| added = of_mut(config).add_from_template(template, name))?;
    Ok(added)
}

/// The copy is selected and does not get the key. `None` when there is no `id`.
pub(super) fn duplicate(cfg: &ConfigStore, id: &str, name: &str) -> Result<Option<String>> {
    let mut copy = None;
    cfg.update(|config| copy = of_mut(config).duplicate(id, name))?;
    Ok(copy)
}

/// Also removes the key, and selects the neighbour when `id` was in use. `false` for the last configuration, which
/// stays, and for an unknown id.
pub(super) fn remove(cfg: &ConfigStore, secrets: &dyn SecretStore, id: &str) -> Result<bool> {
    Ok(cfg.remove_cloud_asr_configuration(id, secrets)?)
}

pub(super) struct KeyChange<'a> {
    pub typed: &'a str,
    pub saved: &'a str,
}

/// Touches nothing but configuration `id`: not the selection, not another configuration. A failed save leaves the key
/// as it was. `false` when no configuration has that id any more: it was deleted while its draft was still being
/// checked, and neither its JSON nor its key may come back.
pub(super) fn save_draft(
    cfg: &ConfigStore,
    secrets: &dyn SecretStore,
    id: &str,
    config: Value,
    key: &KeyChange<'_>,
) -> Result<bool> {
    if !of(&cfg.get()).items.iter().any(|configuration| configuration.id == id) {
        return Ok(false);
    }
    let account = dito_core::secrets::asr_account(id);
    let key_changed = key.typed != key.saved;
    if key_changed {
        write_key(secrets, &account, key.typed)?;
    }
    let written = cfg.update(|current| {
        if let Some(configuration) = of_mut(current).items.iter_mut().find(|configuration| configuration.id == id) {
            configuration.config = config;
        }
    });
    if let Err(error) = written {
        if key_changed && let Err(restore) = write_key(secrets, &account, key.saved) {
            tracing::warn!(%restore, "could not restore the previous speech key");
        }
        return Err(error.into());
    }
    Ok(true)
}

/// An emptied key is removed instead of stored.
fn write_key(secrets: &dyn SecretStore, account: &str, key: &str) -> Result<()> {
    if key.is_empty() { secrets.delete(account) } else { secrets.set(account, key) }?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use dito_core::secrets::MemorySecrets;
    use serde_json::json;

    use super::*;

    fn store() -> (tempfile::TempDir, ConfigStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::open(dir.path().join("config.toml")).unwrap();
        (dir, store)
    }

    fn ids(cfg: &ConfigStore) -> Vec<String> {
        of(&cfg.get()).items.iter().map(|configuration| configuration.id.clone()).collect()
    }

    fn config_of(cfg: &ConfigStore, id: &str) -> Value {
        let saved = cfg.get();
        let configuration = of(&saved).items.iter().find(|configuration| configuration.id == id).cloned();
        configuration.unwrap().config
    }

    #[test]
    fn every_change_reaches_the_file_and_names_its_configuration_by_id() {
        let (dir, cfg) = store();
        let added = add(&cfg, CloudAsrTemplate::ALL[1], "Mine").unwrap();
        assert!(added.starts_with("qwen_asr-"), "{added}");
        let copy = duplicate(&cfg, "openrouter", "Copy").unwrap().unwrap();
        assert_eq!(duplicate(&cfg, "missing", "Copy").unwrap(), None);
        rename(&cfg, &added, "Renamed").unwrap();
        rename(&cfg, &added, "   ").unwrap();
        move_to(&cfg, &copy, 0).unwrap();
        select(&cfg, "qwen_asr").unwrap();

        // The file holds what the store holds: the first change saved the templates, the rest followed.
        let on_disk = ConfigStore::open(dir.path().join("config.toml")).unwrap().get();
        let configurations = on_disk.asr.cloud_asr.as_ref().unwrap();
        let named: Vec<_> = configurations.items.iter().map(|p| (p.id.as_str(), p.name.as_str())).collect();
        assert_eq!(
            named,
            [
                (copy.as_str(), "Copy"),
                ("funasr", "FunASR"),
                ("qwen_asr", "Qwen ASR"),
                ("openrouter", "OpenRouter"),
                (added.as_str(), "Renamed"),
            ],
            "a blank name was refused, the copy moved to the front"
        );
        assert_eq!(configurations.selected, "qwen_asr");
    }

    #[test]
    fn a_draft_lands_in_its_own_configuration_whatever_the_selection_and_the_order() {
        let (_dir, cfg) = store();
        let secrets = MemorySecrets::default();
        secrets.set("asr.qwen_asr", "other key").unwrap();
        move_to(&cfg, "funasr", 2).unwrap();
        select(&cfg, "qwen_asr").unwrap();
        let before = cfg.get().asr.cloud_asr.clone().unwrap();

        let mut draft = config_of(&cfg, "funasr");
        draft["connection"]["model"] = json!("paraformer-v2");
        let key = KeyChange { typed: "typed key", saved: "" };
        assert!(save_draft(&cfg, &secrets, "funasr", draft.clone(), &key).unwrap());

        let after = cfg.get().asr.cloud_asr.clone().unwrap();
        assert_eq!(config_of(&cfg, "funasr"), draft);
        assert_eq!(after.selected, before.selected, "saving a draft never selects its configuration");
        assert_eq!(ids(&cfg), before.items.iter().map(|p| p.id.clone()).collect::<Vec<_>>(), "nor reorders");
        for configuration in before.items.iter().filter(|p| p.id != "funasr") {
            assert_eq!(
                config_of(&cfg, &configuration.id),
                configuration.config,
                "{} was not touched",
                configuration.id
            );
        }
        assert_eq!(secrets.get("asr.funasr").as_deref(), Some("typed key"));
        assert_eq!(secrets.get("asr.qwen_asr").as_deref(), Some("other key"));

        // An emptied key leaves the Keychain; an unchanged key is not written at all.
        let emptied = KeyChange { typed: "", saved: "typed key" };
        assert!(save_draft(&cfg, &secrets, "funasr", draft.clone(), &emptied).unwrap());
        assert_eq!(secrets.get("asr.funasr"), None);
        secrets.set("asr.funasr", "kept").unwrap();
        let unchanged = KeyChange { typed: "same", saved: "same" };
        assert!(save_draft(&cfg, &secrets, "funasr", draft, &unchanged).unwrap());
        assert_eq!(secrets.get("asr.funasr").as_deref(), Some("kept"));
    }

    #[test]
    fn a_draft_of_a_deleted_configuration_comes_back_neither_as_json_nor_as_key() {
        let (_dir, cfg) = store();
        let secrets = MemorySecrets::default();
        secrets.set("asr.funasr", "key").unwrap();
        assert!(remove(&cfg, &secrets, "funasr").unwrap());
        assert_eq!(secrets.get("asr.funasr"), None);

        let late = KeyChange { typed: "typed just before the delete", saved: "key" };
        assert!(!save_draft(&cfg, &secrets, "funasr", json!({"version": 1}), &late).unwrap());
        assert_eq!(secrets.get("asr.funasr"), None, "the key of a deleted configuration is not written back");
        assert!(!ids(&cfg).contains(&"funasr".to_owned()));
        assert_eq!(cfg.get().asr.cloud_asr.as_ref().unwrap().selected, "qwen_asr", "the neighbour took over");
        assert!(!remove(&cfg, &secrets, "funasr").unwrap(), "an id nobody has is not a removal");
    }

    #[test]
    fn a_save_that_fails_leaves_the_key_as_it_was() {
        let (dir, cfg) = store();
        let secrets = MemorySecrets::default();
        secrets.set("asr.qwen_asr", "old").unwrap();
        // A directory where the file belongs: every write fails.
        std::fs::create_dir(dir.path().join("config.toml")).unwrap();

        let mut draft = config_of(&cfg, "qwen_asr");
        draft["connection"]["model"] = json!("changed");
        let change = KeyChange { typed: "new", saved: "old" };
        assert!(save_draft(&cfg, &secrets, "qwen_asr", draft.clone(), &change).is_err());
        assert_eq!(secrets.get("asr.qwen_asr").as_deref(), Some("old"));
        assert_ne!(config_of(&cfg, "qwen_asr"), draft, "the configuration did not change either");

        let first_key = KeyChange { typed: "new", saved: "" };
        assert!(save_draft(&cfg, &secrets, "openrouter", draft, &first_key).is_err());
        assert_eq!(secrets.get("asr.openrouter"), None, "a key that did not exist before is gone again");
    }
}
