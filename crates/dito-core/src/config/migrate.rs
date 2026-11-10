//! Versioned upgrades of `config.toml` (ADR-0015). Each migration rewrites the parsed TOML table from one schema
//! version to the next, before the table becomes a typed `Config`; serde defaults only describe the current shape.

use toml::{Table, Value};

use crate::i18n::Lang;
use crate::prompts::{Style, default_set};

/// Version of the persisted shape that this build reads and writes. Raise it together with appending one function to
/// `MIGRATIONS`, and never edit or renumber a shipped migration: files in the field have already passed through it.
pub const SCHEMA_VERSION: u32 = 2;

const VERSION_KEY: &str = "schema_version";

/// `MIGRATIONS[n]` upgrades a table from version `n` to version `n + 1`. The array length is tied to `SCHEMA_VERSION`,
/// so the compiler rejects a version bump that comes without its migration.
const MIGRATIONS: [fn(&mut Table); SCHEMA_VERSION as usize] = [herdr_defaults, asr_profile_ids];

/// Why a table was not upgraded. Either way the file must stay as it is.
#[derive(Debug, PartialEq)]
pub(super) enum UpgradeError {
    /// `schema_version` is present but is not a version number.
    Malformed(toml::de::Error),
    /// The file was written by a newer build; reading it as the current shape would lose what that build added.
    Newer(u32),
}

/// Upgrade `table` in place to `SCHEMA_VERSION` and return the version it was at. A table without `schema_version`
/// predates versioning and counts as version 0. A table that is refused is left untouched.
pub(super) fn upgrade(table: &mut Table) -> Result<u32, UpgradeError> {
    let found = match table.get(VERSION_KEY) {
        None => 0,
        Some(value) => value.as_integer().and_then(|n| u32::try_from(n).ok()).ok_or_else(malformed)?,
    };
    if found > SCHEMA_VERSION {
        return Err(UpgradeError::Newer(found));
    }
    for migration in &MIGRATIONS[found as usize..] {
        migration(table);
    }
    table.insert(VERSION_KEY.into(), SCHEMA_VERSION.into());
    Ok(found)
}

fn malformed() -> UpgradeError {
    let reason = format!("{VERSION_KEY} must be a whole number from 0 to {}", u32::MAX);
    UpgradeError::Malformed(<toml::de::Error as serde::de::Error>::custom(reason))
}

/// 0 to 1: prompt sets saved before the Herdr style existed get its default text. A set that already has the style
/// keeps it, and a set the file does not have is left to the defaults of the typed load.
fn herdr_defaults(table: &mut Table) {
    let Some(prompts) = table.get_mut("prompts").and_then(Value::as_table_mut) else { return };
    let herdr = Style::builtin("herdr").expect("herdr is a built-in style");
    for (key, lang) in [("ja", Lang::Ja), ("en", Lang::En), ("zh", Lang::Zh)] {
        let Some(set) = prompts.get_mut(key).and_then(Value::as_table_mut) else { continue };
        if !set.contains_key("herdr") {
            let prompt =
                Value::try_from(default_set(lang).style(&herdr)).expect("a style prompt encodes as a TOML table");
            set.insert("herdr".into(), prompt);
        }
    }
}

/// What each position of a version 1 profile list stood for: id, display name and the bundled template that completes
/// a saved list lacking it. Spelled out here instead of read from `AsrProfiles`, so this step keeps its result when the
/// bundled set grows or a name changes.
const V1_PROFILES: &[(&str, &str, &str)] = &[
    ("funasr", "FunASR", include_str!("../../../../design/asr-templates/funasr.json")),
    ("qwen_asr", "Qwen ASR", include_str!("../../../../design/asr-templates/qwen-asr.json")),
    ("openrouter", "OpenRouter", include_str!("../../../../design/asr-templates/openrouter.json")),
];

/// 1 to 2: cloud speech profiles are identified by id instead of position, so a bundled template can be added without
/// moving saved ones. In every `cloud_asr` table, `selected` becomes the id of the profile it pointed at and each saved
/// JSON string becomes an `{ id, name, config }` table with the string untouched. A list that lacks a bundled profile
/// gets it from its template, entries past the third (never written by the app) are dropped, and a selection that
/// points nowhere becomes the first profile, whose Keychain account the old code already fell back to. A table that is
/// not in the old shape is left for the typed load to accept or reject.
fn asr_profile_ids(table: &mut Table) {
    for section in ["asr", "polish"] {
        let cloud = table
            .get_mut(section)
            .and_then(Value::as_table_mut)
            .and_then(|engine| engine.get_mut("cloud_asr"))
            .and_then(Value::as_table_mut);
        if let Some(cloud) = cloud {
            identify_profiles(cloud);
        }
    }
}

fn identify_profiles(cloud: &mut Table) {
    let selected = match cloud.get("selected") {
        None => Some(0),
        Some(Value::Integer(position)) => usize::try_from(*position).ok(),
        Some(_) => return,
    };
    let profiles = match cloud.get("profiles") {
        None => None,
        Some(Value::Array(saved)) if saved.iter().all(Value::is_str) => Some(
            V1_PROFILES
                .iter()
                .enumerate()
                .map(|(position, &(id, name, template))| {
                    let config = saved.get(position).and_then(Value::as_str).unwrap_or(template);
                    let mut profile = Table::new();
                    profile.insert("id".into(), id.into());
                    profile.insert("name".into(), name.into());
                    profile.insert("config".into(), config.into());
                    Value::Table(profile)
                })
                .collect::<Vec<_>>(),
        ),
        Some(_) => return,
    };
    let id = selected.and_then(|position| V1_PROFILES.get(position)).unwrap_or(&V1_PROFILES[0]).0;
    cloud.insert("selected".into(), id.into());
    if let Some(profiles) = profiles {
        cloud.insert("profiles".into(), Value::Array(profiles));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AsrProfiles;
    use crate::prompts::StylePrompt;

    fn parse(text: &str) -> Table {
        text.parse().unwrap()
    }

    #[test]
    fn version_zero_gains_default_herdr_prompts_only_where_missing() {
        let mut legacy = parse(
            r#"
onboarded = true

[prompts.ja]
base = "my rule"

[prompts.en.herdr]
instruction = "my herdr rule"
samples = ["short"]
"#,
        );
        assert_eq!(upgrade(&mut legacy), Ok(0));
        assert_eq!(legacy[VERSION_KEY].as_integer(), Some(i64::from(SCHEMA_VERSION)));
        assert_eq!(legacy["onboarded"].as_bool(), Some(true), "unrelated settings are untouched");
        let prompts = &legacy["prompts"];
        let herdr = |lang: &str| -> StylePrompt { prompts[lang]["herdr"].clone().try_into().unwrap() };
        assert_eq!(&herdr("ja"), default_set(Lang::Ja).style(&Style::builtin("herdr").unwrap()));
        assert_eq!(prompts["ja"]["base"].as_str(), Some("my rule"), "edits next to the new style stay");
        assert_eq!(herdr("en"), StylePrompt { instruction: "my herdr rule".into(), samples: vec!["short".into()] });
        assert!(prompts.get("zh").is_none(), "a set the file lacks is left to the typed defaults");
    }

    #[test]
    fn version_one_cloud_asr_profiles_get_ids_keep_their_text_and_gain_the_missing_bundled_one() {
        let first = r#"{"connection":{"base_url":"https://a.example","model":"m1"}}"#;
        // Spacing and key order the app never writes: only an untouched string stays equal to it.
        let second = r#"{ "language_map": {"ja": "JP", "auto": null},  "connection": {"model": "m2"} }"#;
        let mut legacy = parse(&format!(
            r#"
schema_version = 1
onboarded = true

[asr.cloud_asr]
selected = 1
profiles = ['{first}', '{second}']
"#
        ));

        assert_eq!(upgrade(&mut legacy), Ok(1));
        assert_eq!(legacy[VERSION_KEY].as_integer(), Some(i64::from(SCHEMA_VERSION)));
        assert_eq!(legacy["onboarded"].as_bool(), Some(true), "unrelated settings are untouched");
        let cloud = &legacy["asr"]["cloud_asr"];
        assert_eq!(cloud["selected"].as_str(), Some("qwen_asr"), "position 1 becomes the id at position 1");
        let profiles = cloud["profiles"].as_array().unwrap();
        let identities: Vec<_> =
            profiles.iter().map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap())).collect();
        assert_eq!(identities, [("funasr", "FunASR"), ("qwen_asr", "Qwen ASR"), ("openrouter", "OpenRouter")]);
        assert_eq!(profiles[0]["config"].as_str(), Some(first), "a saved profile keeps its text byte for byte");
        assert_eq!(profiles[1]["config"].as_str(), Some(second), "an edited profile keeps its text byte for byte");
        let completed: serde_json::Value = serde_json::from_str(profiles[2]["config"].as_str().unwrap()).unwrap();
        assert_eq!(completed, AsrProfiles::default().profiles[2].config, "a missing profile starts from its template");
    }

    #[test]
    fn version_one_selection_that_points_nowhere_becomes_the_first_profile_and_extra_entries_are_dropped() {
        let saved = r#"profiles = ['{"a":1}', '{"b":2}', '{"c":3}', '{"extra":true}']"#;
        for selected in ["3", "-1"] {
            let mut legacy =
                parse(&format!("schema_version = 1\n[polish.cloud_asr]\nselected = {selected}\n{saved}\n"));
            assert_eq!(upgrade(&mut legacy), Ok(1));
            let cloud = &legacy["polish"]["cloud_asr"];
            assert_eq!(cloud["selected"].as_str(), Some("funasr"), "selected = {selected}");
            let configs: Vec<_> =
                cloud["profiles"].as_array().unwrap().iter().map(|p| p["config"].as_str().unwrap()).collect();
            assert_eq!(configs, [r#"{"a":1}"#, r#"{"b":2}"#, r#"{"c":3}"#], "selected = {selected}");
        }
    }

    #[test]
    fn newer_or_malformed_versions_are_refused_and_leave_the_table_alone() {
        let mut newer = parse("schema_version = 999\n[general]\nshortcut = \"x\"\n");
        let before = newer.clone();
        assert_eq!(upgrade(&mut newer), Err(UpgradeError::Newer(999)));
        assert_eq!(newer, before);

        for bad in ["-1", "4294967296", "\"1\"", "1.5", "true"] {
            let mut table = parse(&format!("schema_version = {bad}\n[general]\nshortcut = \"x\"\n"));
            let before = table.clone();
            assert!(matches!(upgrade(&mut table), Err(UpgradeError::Malformed(_))), "{bad}");
            assert_eq!(table, before, "{bad}");
        }
    }
}
