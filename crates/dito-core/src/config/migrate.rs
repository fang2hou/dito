//! Versioned upgrades of `config.toml` (ADR-0015). Each migration rewrites the parsed TOML table from one schema
//! version to the next, before the table becomes a typed `Config`; serde defaults only describe the current shape.

use toml::{Table, Value};

use crate::i18n::Lang;
use crate::prompts::default_set;

/// Version of the persisted shape that this build reads and writes. Raise it together with appending one function to
/// `MIGRATIONS`, and never edit or renumber a shipped migration: files in the field have already passed through it.
pub const SCHEMA_VERSION: u32 = 1;

const VERSION_KEY: &str = "schema_version";

/// `MIGRATIONS[n]` upgrades a table from version `n` to version `n + 1`. The array length is tied to `SCHEMA_VERSION`,
/// so the compiler rejects a version bump that comes without its migration.
const MIGRATIONS: [fn(&mut Table); SCHEMA_VERSION as usize] = [herdr_defaults];

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
    for (key, lang) in [("ja", Lang::Ja), ("en", Lang::En), ("zh", Lang::Zh)] {
        let Some(set) = prompts.get_mut(key).and_then(Value::as_table_mut) else { continue };
        if !set.contains_key("herdr") {
            let herdr = Value::try_from(default_set(lang).herdr).expect("a style prompt encodes as a TOML table");
            set.insert("herdr".into(), herdr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(herdr("ja"), default_set(Lang::Ja).herdr);
        assert_eq!(prompts["ja"]["base"].as_str(), Some("my rule"), "edits next to the new style stay");
        assert_eq!(herdr("en"), StylePrompt { instruction: "my herdr rule".into(), samples: vec!["short".into()] });
        assert!(prompts.get("zh").is_none(), "a set the file lacks is left to the typed defaults");
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
