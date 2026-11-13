//! Versioned upgrades of `config.toml` (ADR-0015). Each migration rewrites the parsed TOML table before it becomes a
//! typed `Config`, because serde defaults only describe the current shape.

use toml::{Table, Value};

use super::DisplayLanguage;
use crate::i18n::Lang;
use crate::prompts::{AppTarget, default_set};

/// Version of the persisted shape that this build reads and writes. Raise it together with appending one function to
/// `MIGRATIONS`, and never edit or renumber a shipped migration: files in the field have already passed through it.
pub const SCHEMA_VERSION: u32 = 6;

const VERSION_KEY: &str = "schema_version";

/// `MIGRATIONS[n]` upgrades a table from version `n` to version `n + 1`. The array length is tied to `SCHEMA_VERSION`,
/// so the compiler rejects a version bump that comes without its migration.
const MIGRATIONS: [fn(&mut Table); SCHEMA_VERSION as usize] =
    [herdr_defaults, asr_profile_ids, cloud_configurations, wordbook_setting, prompt_scaffold, command_settings];

/// Either way the file must stay as it is.
#[derive(Debug, PartialEq)]
pub(super) enum UpgradeError {
    Malformed(toml::de::Error),
    /// Reading it as the current shape would lose what the newer build added.
    Newer(u32),
}

/// A table without `schema_version` predates versioning and counts as version 0. A refused table is left untouched.
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
    let herdr = AppTarget::builtin("herdr").expect("herdr is a built-in style");
    for (key, lang) in [("ja", Lang::Ja), ("en", Lang::En), ("zh", Lang::Zh)] {
        let Some(set) = prompts.get_mut(key).and_then(Value::as_table_mut) else { continue };
        if !set.contains_key("herdr") {
            let prompt =
                Value::try_from(default_set(lang).app_rules(&herdr)).expect("a style prompt encodes as a TOML table");
            set.insert("herdr".into(), prompt);
        }
    }
}

/// What each position of a version 1 configuration list stood for: id, display name and the bundled template that
/// completes a saved list lacking it. Spelled out here instead of read from `CloudAsrConfigurations`, so this step
/// keeps its result when the bundled set grows or a name changes.
const V1_PROFILES: &[(&str, &str, &str)] = &[
    ("funasr", "FunASR", include_str!("../../../../design/asr-templates/funasr.json")),
    ("qwen_asr", "Qwen ASR", include_str!("../../../../design/asr-templates/qwen-asr.json")),
    ("openrouter", "OpenRouter", include_str!("../../../../design/asr-templates/openrouter.json")),
];

/// 1 to 2: cloud speech configurations are identified by id instead of position, so a bundled template can be added
/// without moving saved ones. In every `cloud_asr` table, `selected` becomes the id of the configuration it pointed at
/// and each saved JSON string becomes an `{ id, name, config }` table with the string untouched. A list that lacks a
/// bundled configuration gets it from its template, entries past the third (never written by the app) are dropped, and
/// a selection that points nowhere becomes the first configuration, whose Keychain account the old code already fell
/// back to. A table that is not in the old shape is left for the typed load to accept or reject.
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

/// 2 to 3: cloud speech and cloud polish both keep their saved configurations as `items`. The two steps below do not
/// depend on each other.
fn cloud_configurations(table: &mut Table) {
    speech_items(table);
    polish_configurations(table);
}

/// In every `cloud_asr` table, the list version 2 wrote as `profiles` is renamed `items`, the key the polish
/// configurations use. The entries are moved as they are, so every request JSON string stays byte for byte. A table
/// that already has `items` is left alone.
fn speech_items(table: &mut Table) {
    for section in ["asr", "polish"] {
        let cloud = table
            .get_mut(section)
            .and_then(Value::as_table_mut)
            .and_then(|engine| engine.get_mut("cloud_asr"))
            .and_then(Value::as_table_mut);
        if let Some(cloud) = cloud
            && !cloud.contains_key("items")
            && let Some(list) = cloud.remove("profiles")
        {
            cloud.insert("items".into(), list);
        }
    }
}

/// The three provider forms of the polish engine become cloud polish configurations that carry their own
/// request parameters, so several can be kept and the Bedrock sampling the engine hard-coded becomes a setting.
/// `polish.provider` turns into `polish.cloud_polish`, holding `azure`, `bedrock` and `compat` in that order with
/// `kind` as the selection; the ids keep the Keychain accounts `polish.azure` and `polish.compat` that hold the saved
/// keys. Each configuration keeps its fields and its reasoning effort, and Bedrock gets the temperature and token
/// limit its engine sent before, so a request does not change. A field the file lacks gets what the typed load of
/// version 2 gave it. `asr.provider` was never read by the speech engine and is dropped. A file without
/// `polish.provider` keeps none: the typed load then starts from the templates. The names are spelled out here, as in
/// the step before, so this one keeps its result when a label changes.
fn polish_configurations(table: &mut Table) {
    let lang = display_language(table);
    if let Some(asr) = table.get_mut("asr").and_then(Value::as_table_mut) {
        asr.remove("provider");
    }
    let Some(polish) = table.get_mut("polish").and_then(Value::as_table_mut) else { return };
    let Some(provider) = polish.remove("provider") else { return };

    let none = Table::new();
    let provider = provider.as_table().unwrap_or(&none);
    let form = |kind: &str| provider.get(kind).and_then(Value::as_table).unwrap_or(&none);
    let text = |form: &Table, key: &str, default: &str| -> Value {
        form.get(key).and_then(Value::as_str).unwrap_or(default).into()
    };
    let (azure, bedrock, compat) = (form("azure"), form("bedrock"), form("compat"));
    let items = [
        configuration(
            ("azure", "Azure OpenAI"),
            [("endpoint", text(azure, "endpoint", "")), ("deployment", text(azure, "deployment", ""))],
            [("reasoning_effort", text(azure, "reasoning_effort", "low"))],
        ),
        configuration(
            ("bedrock", "Amazon Bedrock"),
            [
                ("profile", text(bedrock, "profile", "")),
                ("region", text(bedrock, "region", "ap-northeast-1")),
                ("model_id", text(bedrock, "model_id", "")),
            ],
            [
                ("temperature", Value::Float(0.2)),
                ("max_tokens", Value::Integer(2048)),
                ("reasoning_effort", text(bedrock, "reasoning_effort", "")),
            ],
        ),
        configuration(
            ("compat", compat_name(lang)),
            [("base_url", text(compat, "base_url", "")), ("model", text(compat, "model", ""))],
            [("reasoning_effort", text(compat, "reasoning_effort", "low"))],
        ),
    ];
    let selected = provider.get("kind").and_then(Value::as_str).filter(|kind| matches!(*kind, "bedrock" | "compat"));

    let mut cloud = Table::new();
    cloud.insert("selected".into(), selected.unwrap_or("azure").into());
    cloud.insert("items".into(), Value::Array(items.into()));
    polish.insert("cloud_polish".into(), Value::Table(cloud));
}

/// The name of the OpenAI-compatible configuration follows the UI language.
fn display_language(table: &Table) -> Lang {
    table
        .get("general")
        .and_then(Value::as_table)
        .and_then(|general| general.get("display_language"))
        .and_then(|value| value.clone().try_into::<DisplayLanguage>().ok())
        .unwrap_or_default()
        .resolve()
}

fn compat_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "OpenAI 互換 API",
        Lang::En => "OpenAI-compatible API",
        Lang::Zh => "OpenAI 兼容 API",
    }
}

/// One `[[polish.cloud_polish.items]]` table. The template is the id: each configuration is the one its form was.
fn configuration<const FIELDS: usize, const PARAMS: usize>(
    (id, name): (&str, &str),
    fields: [(&str, Value); FIELDS],
    params: [(&str, Value); PARAMS],
) -> Value {
    let mut item = Table::new();
    item.insert("id".into(), id.into());
    item.insert("name".into(), name.into());
    item.insert("template".into(), id.into());
    item.extend(fields.into_iter().map(|(key, value)| (key.to_owned(), value)));
    item.insert(
        "params".into(),
        Value::Table(params.into_iter().map(|(key, value)| (key.to_owned(), value)).collect()),
    );
    Value::Table(item)
}

/// 3 to 4: the spelling-reference toggle moves to the wordbook key.
fn wordbook_setting(table: &mut Table) {
    if let Some(value) = table.remove("dictionary_for_polish") {
        table.entry("wordbook_for_polish").or_insert(value);
    }
}

/// 4 to 5: the task rules moved out of the prompt sets into a fixed scaffold, so a saved set, which still spells
/// them out, would state them twice. Saved prompts are dropped and the typed load starts from the defaults. This was
/// decided before any release; the backup keeps what the user wrote.
fn prompt_scaffold(table: &mut Table) {
    table.remove("prompts");
}

/// 5 to 6: command mode adds `general.command_shortcut` and `general.confirm_before_replace`. A file that has a
/// `general` table gets both written out, so a build that does not know them refuses the file instead of dropping them
/// on its next save. The shortcut is written empty: unset, so it is derived from the dictation shortcut (Shift plus it)
/// and follows when that changes. A file without `general` takes the typed defaults.
fn command_settings(table: &mut Table) {
    let Some(general) = table.get_mut("general").and_then(Value::as_table_mut) else { return };
    general.entry("command_shortcut").or_insert("".into());
    general.entry("confirm_before_replace").or_insert(false.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CloudAsrConfigurations, Config};
    use crate::prompts::AppRules;

    fn parse(text: &str) -> Table {
        text.parse().unwrap()
    }

    #[test]
    fn version_three_moves_the_wordbook_toggle() {
        let mut legacy = parse("schema_version = 3\ndictionary_for_polish = true");
        assert_eq!(upgrade(&mut legacy), Ok(3));
        assert!(!legacy.contains_key("dictionary_for_polish"));
        let config: Config = legacy.try_into().unwrap();
        assert!(config.wordbook_for_polish);
    }

    #[test]
    fn version_four_drops_saved_prompts_and_nothing_else() {
        let mut legacy = parse(
            "schema_version = 4\nonboarded = true\n[prompts.ja]\nbase = \"my rule\"\n[prompts.applications]\n\"example.editor\" = \"Editor\"",
        );
        assert_eq!(upgrade(&mut legacy), Ok(4));
        let config: Config = legacy.try_into().unwrap();
        assert!(config.onboarded);
        assert_eq!(config.prompts, crate::prompts::Prompts::default());
    }

    #[test]
    fn version_five_gains_the_command_settings_with_an_unset_shortcut() {
        let general = |shortcut: &str| {
            let mut legacy = parse(&format!("schema_version = 5\n[general]\nshortcut = '{shortcut}'\n"));
            assert_eq!(upgrade(&mut legacy), Ok(5));
            assert_eq!(legacy["general"]["command_shortcut"].as_str(), Some(""), "written out, but unset");
            legacy.try_into::<Config>().unwrap().general
        };
        for shortcut in ["RightCommand", "RightOption", "Fn"] {
            let migrated = general(shortcut);
            assert_eq!((migrated.shortcut.as_str(), migrated.command_shortcut.as_str()), (shortcut, ""));
            assert!(!migrated.confirm_before_replace);
        }

        let mut saved = parse("schema_version = 5\n[general]\nshortcut = 'Fn'\ncommand_shortcut = 'LeftCommand'\n");
        upgrade(&mut saved).unwrap();
        assert_eq!(saved["general"]["command_shortcut"].as_str(), Some("LeftCommand"), "a saved value stays");
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
        herdr_defaults(&mut legacy);
        assert_eq!(legacy["onboarded"].as_bool(), Some(true), "unrelated settings are untouched");
        let prompts = &legacy["prompts"];
        let herdr = |lang: &str| -> AppRules { prompts[lang]["herdr"].clone().try_into().unwrap() };
        assert_eq!(&herdr("ja"), default_set(Lang::Ja).app_rules(&AppTarget::builtin("herdr").unwrap()));
        assert_eq!(prompts["ja"]["base"].as_str(), Some("my rule"), "edits next to the new style stay");
        assert_eq!(herdr("en"), AppRules { instruction: "my herdr rule".into(), samples: vec!["short".into()] });
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

        // Only this step: the later ones rename the list this one writes.
        asr_profile_ids(&mut legacy);
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
        assert_eq!(
            completed,
            CloudAsrConfigurations::default().items[2].config,
            "a missing profile starts from its template"
        );
    }

    #[test]
    fn version_one_selection_that_points_nowhere_becomes_the_first_profile_and_extra_entries_are_dropped() {
        let saved = r#"profiles = ['{"a":1}', '{"b":2}', '{"c":3}', '{"extra":true}']"#;
        for selected in ["3", "-1"] {
            let mut legacy =
                parse(&format!("schema_version = 1\n[polish.cloud_asr]\nselected = {selected}\n{saved}\n"));
            asr_profile_ids(&mut legacy);
            let cloud = &legacy["polish"]["cloud_asr"];
            assert_eq!(cloud["selected"].as_str(), Some("funasr"), "selected = {selected}");
            let configs: Vec<_> =
                cloud["profiles"].as_array().unwrap().iter().map(|p| p["config"].as_str().unwrap()).collect();
            assert_eq!(configs, [r#"{"a":1}"#, r#"{"b":2}"#, r#"{"c":3}"#], "selected = {selected}");
        }
    }

    /// What version 2 wrote for cloud speech: a `profiles` list of `{ id, name, config }` tables, the config as the
    /// compact JSON the app saves. The polish section can carry a list too, left over from before polishing had its
    /// own.
    const V2_SPEECH: &str = r#"
schema_version = 2
onboarded = true

[asr]
source = "provider"

[asr.cloud_asr]
selected = "qwen_asr-3c9e"

[[asr.cloud_asr.profiles]]
id = "funasr"
name = "FunASR"
config = '{"connection":{"base_url":"https://a.example/v1","model":"paraformer-v2"},"version":1}'

[[asr.cloud_asr.profiles]]
id = "qwen_asr-3c9e"
name = "Mine"
config = '{"connection":{"model":"my-model"},"language_map":{"auto":null,"ja":"JP"},"version":1}'

[polish.cloud_asr]
selected = "openrouter"

[[polish.cloud_asr.profiles]]
id = "openrouter"
name = "OpenRouter"
config = '{"connection":{"model":"m3"},"version":1}'
"#;

    #[test]
    fn version_two_cloud_speech_list_moves_to_items_with_every_entry_and_request_json_untouched() {
        let mut legacy = parse(V2_SPEECH);
        let before = [
            ("asr", legacy["asr"]["cloud_asr"]["profiles"].clone()),
            ("polish", legacy["polish"]["cloud_asr"]["profiles"].clone()),
        ];
        assert_eq!(upgrade(&mut legacy), Ok(2));
        assert_eq!(legacy["onboarded"].as_bool(), Some(true), "unrelated settings are untouched");
        for (section, entries) in before {
            let cloud = legacy[section]["cloud_asr"].as_table().unwrap();
            assert!(cloud.get("profiles").is_none(), "{section}");
            assert_eq!(cloud["items"], entries, "{section}: the same entries, each request JSON byte for byte");
        }
        assert_eq!(legacy["asr"]["cloud_asr"]["selected"].as_str(), Some("qwen_asr-3c9e"));
        assert_eq!(legacy["polish"]["cloud_asr"]["selected"].as_str(), Some("openrouter"));

        let config: Config = legacy.clone().try_into().unwrap();
        let speech = config.asr.cloud_asr.as_ref().unwrap();
        let identities: Vec<_> = speech.items.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect();
        assert_eq!(identities, [("funasr", "FunASR"), ("qwen_asr-3c9e", "Mine")]);
        assert_eq!(speech.selected, "qwen_asr-3c9e");
        assert!(speech.active().unwrap().config["language_map"]["auto"].is_null(), "a null survives");
        let saved: Table = toml::to_string(&config).unwrap().parse().unwrap();
        assert_eq!(saved["asr"]["cloud_asr"]["items"], legacy["asr"]["cloud_asr"]["items"]);
        assert_eq!(saved["polish"]["cloud_asr"]["items"], legacy["polish"]["cloud_asr"]["items"]);
    }

    #[test]
    fn a_version_one_cloud_speech_list_ends_up_under_items_too() {
        let mut legacy = parse("schema_version = 1\n[asr.cloud_asr]\nselected = 1\nprofiles = ['{\"a\":1}']\n");
        assert_eq!(upgrade(&mut legacy), Ok(1));
        let cloud = legacy["asr"]["cloud_asr"].as_table().unwrap();
        assert!(cloud.get("profiles").is_none());
        let configs: Vec<_> =
            cloud["items"].as_array().unwrap().iter().map(|c| c["config"].as_str().unwrap()).collect();
        assert_eq!(configs[0], r#"{"a":1}"#, "the saved text is the first entry, untouched");
        assert_eq!(configs.len(), 3, "the missing bundled entries are completed");
        assert_eq!(cloud["selected"].as_str(), Some("qwen_asr"));
    }

    /// What version 2 wrote for the polish engine of a user who filled in all three forms.
    const V2_POLISH: &str = r#"
schema_version = 2
onboarded = true

[general]
shortcut = "LeftOption"
display_language = "en"

[asr]
source = "local"
local_model = "sensevoice-small"

[asr.provider]
kind = "compat"

[asr.provider.azure]
endpoint = ""
deployment = ""
reasoning_effort = "low"

[asr.provider.compat]
base_url = "http://asr.example/v1"
model = "whisper"
reasoning_effort = "low"

[polish]
enabled = true
source = "provider"
local_model = "qwen3.5-2b-instruct"

[polish.provider]
kind = "bedrock"

[polish.provider.azure]
endpoint = "https://dito.openai.azure.com/"
deployment = "gpt-6.1-terra"
reasoning_effort = "high"

[polish.provider.bedrock]
profile = "work"
region = "us-east-1"
model_id = "anthropic.claude-sonnet-4-5"
reasoning_effort = "low"

[polish.provider.compat]
base_url = "https://router.example/v1"
model = "vendor/model"
reasoning_effort = ""
"#;

    #[test]
    fn version_two_provider_forms_become_three_configurations_that_behave_the_same() {
        let mut legacy = parse(V2_POLISH);
        assert_eq!(upgrade(&mut legacy), Ok(2));
        assert_eq!(legacy[VERSION_KEY].as_integer(), Some(i64::from(SCHEMA_VERSION)));
        assert_eq!(legacy["onboarded"].as_bool(), Some(true), "unrelated settings are untouched");
        assert_eq!(legacy["general"]["shortcut"].as_str(), Some("LeftOption"));
        assert_eq!(legacy["polish"]["source"].as_str(), Some("provider"));
        assert_eq!(legacy["asr"]["local_model"].as_str(), Some("sensevoice-small"));
        assert!(legacy["asr"].get("provider").is_none(), "speech never read the provider forms");
        assert!(legacy["polish"].get("provider").is_none());

        let cloud = &legacy["polish"]["cloud_polish"];
        assert_eq!(cloud["selected"].as_str(), Some("bedrock"), "the old kind stays selected");
        let items = cloud["items"].as_array().unwrap();
        let identities: Vec<_> =
            items.iter().map(|c| (c["id"].as_str().unwrap(), c["name"].as_str().unwrap())).collect();
        assert_eq!(
            identities,
            [("azure", "Azure OpenAI"), ("bedrock", "Amazon Bedrock"), ("compat", "OpenAI-compatible API")]
        );
        let efforts: Vec<_> = items.iter().map(|c| c["params"]["reasoning_effort"].as_str().unwrap()).collect();
        assert_eq!(efforts, ["high", "low", ""], "each form keeps its reasoning effort");
        let bedrock = items[1]["params"].as_table().unwrap();
        assert_eq!(bedrock["temperature"].as_float(), Some(0.2), "the sampling the engine hard-coded");
        assert_eq!(bedrock["max_tokens"].as_integer(), Some(2048));
        for plain in [&items[0], &items[2]] {
            let keys: Vec<_> = plain["params"].as_table().unwrap().keys().collect();
            assert_eq!(keys, ["reasoning_effort"], "nothing else is sent that was not sent before");
        }

        let config: Config = legacy.try_into().unwrap();
        let configurations = config.polish.engine.polish_configurations(config.general.lang());
        let target = |id: &str| configurations.get(id).unwrap().target.clone();
        assert_eq!(
            target("azure"),
            crate::config::CloudPolishTarget::Azure {
                endpoint: "https://dito.openai.azure.com/".into(),
                deployment: "gpt-6.1-terra".into()
            }
        );
        assert_eq!(
            target("bedrock"),
            crate::config::CloudPolishTarget::Bedrock {
                profile: "work".into(),
                region: "us-east-1".into(),
                model_id: "anthropic.claude-sonnet-4-5".into()
            }
        );
        assert_eq!(
            target("compat"),
            crate::config::CloudPolishTarget::Compat {
                base_url: "https://router.example/v1".into(),
                model: "vendor/model".into()
            }
        );
        assert_eq!(config.polish.engine.label(), "bedrock:anthropic.claude-sonnet-4-5", "history labels do not change");
    }

    #[test]
    fn version_two_defaults_become_the_configurations_of_a_fresh_install() {
        let mut legacy = parse(
            r#"
schema_version = 2
[general]
display_language = "ja"
[polish]
enabled = true
[polish.provider]
kind = "azure"
[polish.provider.azure]
endpoint = ""
deployment = ""
reasoning_effort = "low"
[polish.provider.bedrock]
profile = ""
region = "ap-northeast-1"
model_id = ""
reasoning_effort = ""
[polish.provider.compat]
base_url = ""
model = ""
reasoning_effort = "low"
"#,
        );
        assert_eq!(upgrade(&mut legacy), Ok(2));
        let config: Config = legacy.try_into().unwrap();
        assert_eq!(config.polish.engine.cloud_polish, Some(crate::config::CloudPolishConfigurations::new(Lang::Ja)));
    }

    #[test]
    fn the_compatible_name_follows_the_files_language_and_odd_provider_tables_fall_back_to_the_old_defaults() {
        let names = |general: &str| -> String {
            let mut table = parse(&format!("schema_version = 2\n{general}\n[polish.provider]\n"));
            upgrade(&mut table).unwrap();
            table["polish"]["cloud_polish"]["items"][2]["name"].as_str().unwrap().to_owned()
        };
        assert_eq!(names("[general]\ndisplay_language = 'ja'"), "OpenAI 互換 API");
        assert_eq!(names("[general]\ndisplay_language = 'en'"), "OpenAI-compatible API");
        assert_eq!(names("[general]\ndisplay_language = 'zh'"), "OpenAI 兼容 API");
        let system = compat_name(DisplayLanguage::System.resolve());
        assert_eq!(names("[general]\ndisplay_language = 'system'"), system);
        assert_eq!(names(""), system, "a file without the setting follows the system");

        // An empty or unreadable provider table is what the typed load of version 2 made of it.
        for provider in ["[polish.provider]\nkind = 'nowhere'", "[polish]\nprovider = 5"] {
            let mut table = parse(&format!("schema_version = 2\n{provider}\n"));
            upgrade(&mut table).unwrap();
            let config: Config = table.try_into().unwrap();
            assert_eq!(config.polish.engine.cloud_polish.unwrap().selected, "azure", "{provider}");
        }
        let mut without = parse("schema_version = 2\n[polish]\nenabled = false\n");
        upgrade(&mut without).unwrap();
        assert!(without["polish"].get("cloud_polish").is_none(), "nothing saved, so the templates stand in");
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
