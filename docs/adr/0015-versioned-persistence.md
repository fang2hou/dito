# ADR-0015: Persisted formats are versioned and migrated before loading

- **Status**: accepted
- **Date**: 2026-11-10

## Context

`config.toml` outlived several shapes, and each upgrade was handled by a fix-up inside a `Deserialize` impl, such as inserting a missing prompt style while parsing. These patches are invisible at the file level, run on every load, cannot be tested apart from the type, and give no way to tell which shape a file has. A build that meets a file written by a newer version had no way to notice, so it could read the file as if nothing had changed and overwrite it on the next save.

## Decision

Every persisted format carries a schema version. A change to a persisted shape goes through an ordered, tested migration that runs on the raw data before typed loading. A migration never destroys the previous file: the original is backed up first, then the migrated file is written. Data written by a newer build is refused, not reinterpreted, and the file is left untouched. Fix-ups for old shapes inside `Deserialize` impls are not allowed; serde defaults only describe the current shape.

`config.toml` follows this now: a top-level `schema_version`, with a missing key meaning version 0. `ConfigStore::open` parses the file into a TOML table, runs the migrations in order, saves the original as `config.toml.v<old>.bak` before rewriting, and fails with `ConfigError::NewerSchema` for a version this build does not know. The SQLite store follows the same rule, with `PRAGMA user_version` as its version and ordered migrations run before the store is used.

## Schema versions

The `config.toml` shapes so far, one migration each (`MIGRATIONS` in `migrate.rs`):

| Version | What changed from the one before |
| --- | --- |
| 1 | Prompt sets gain the Herdr style with its default text |
| 2 | Cloud speech configurations are identified by id instead of position (the list was then named `cloud_asr.profiles`) |
| 3 | The three provider forms become named cloud polish configurations (`polish.cloud_polish`) with request parameters, the Bedrock temperature and token limit that were hard-coded become its parameters, `asr.provider` is dropped, and the cloud speech list `cloud_asr.profiles` is renamed `cloud_asr.items`, as in `polish.cloud_polish`, with its entries unchanged |
| 4 | Rename the spelling-reference toggle to `wordbook_for_polish` |
| 5 | Saved prompts are dropped and the defaults take their place, because the task rules moved from the prompt sets into the fixed scaffold (ADR-0012). Decided before any release; the backup keeps the old text |
| 6 | Command mode: `general.command_shortcut` (empty, meaning unset: derived as Shift plus the dictation shortcut, so it follows when that changes) and `general.confirm_before_replace` (off) are written into an existing `general` table |
| 7 | The model settings leave `polish` for their own table `text_model`, and `cloud_polish` is renamed `cloud_text`, because the model that polishes also runs commands and finds words. `polish` keeps `enabled`. Keychain accounts keep the `polish.` prefix |
| 8 | The Tools window gains a shortcut: `general.tools_shortcut` is written into an existing `general` table with its default, `Control+Option+T` |
| 9 | Transcription gains generated titles: `transcription.auto_title` (on) and `transcription.title_template` (the default instruction) are written into an existing `transcription` table |
| 10 | The default `transcription.title_template` becomes the language-agnostic English instruction; user-edited instructions stay unchanged |
| 11 | `general.display_language` adds `zh_hant`, `ko`, `mn`, and `fr`; existing values stay unchanged, and the version bump lets older builds report `NewerSchema` cleanly |
| 12 | The Local API service gains its `local_api` table: `enabled` (off), `port` (17841), `api_any_token` (off) and `mcp_any_token` (on). The access token is a secret and stays in the Keychain |

Version 3 keeps the Keychain accounts `polish.azure` and `polish.compat` working by giving the migrated configurations the ids `azure`, `bedrock` and `compat`; every later configuration has a random id suffix, so the key of a removed one is never picked up by a new one.

The SQLite store counts its migrations in `PRAGMA user_version` (`MIGRATIONS` in `store/schema.rs`):

| Version | What changed from the one before |
| --- | --- |
| 1 | The schema from before versioning, adopted unchanged |
| 2 | `history` gains the engine labels `asr_engine` and `polish_engine` |
| 3 | The wordbook table is renamed in place and keeps its rows |
| 4 | `history` gains `asr_ms` and `polish_ms` (step times), `source` (the text a command worked on) and `command` (the details of a command run as JSON); all nullable, so earlier rows are unchanged |
| 7 | Chat conversations: `chats` and `chat_messages`. A message keeps the files and meetings it carried as a JSON list in `attachments`, with their text copied in |

Version 4 stores the details of a command in one JSON column, `CommandDetails`. Its shape is a persisted shape like any column: a change to it is a new migration of the stored JSON, and `CommandDetails` gets no serde defaults to paper over old rows. A row whose details a build cannot read (written by a newer build) still lists, without them.

`chat_messages.attachments` follows the same rule: the `Attachment` shape gets no serde defaults, a change to it is a new migration of the stored JSON, and a message whose attachments a build cannot read still lists, without them. A reply found `generating` when the store opens was cut short and is set to `stopped` with its words.

## Alternatives Considered

### Tolerant `Deserialize` impls and serde defaults only

- Pros: No version field; old files load without a rewrite
- Cons: Shapes cannot be told apart, renames and removals cannot be expressed, fixes run on every load and are not tested as steps
- Why not chosen: This is the state that caused the problem

### Rewrite in place without a backup

- Pros: Simpler; no leftover files
- Cons: A faulty migration or a downgrade loses the user's settings with no way back
- Why not chosen: Rejected; settings and prompts are user-written content

### Reset to defaults when the file cannot be read

- Pros: The app always starts
- Cons: Silently discards the user's configuration
- Why not chosen: Rejected; an unreadable or newer file is reported and kept

## Consequences

Each shape change costs one migration function and one test, and the schema version is a visible fact in the file. Downgrading to an older build after a migration is refused rather than corrupting data; the user's previous file remains as the backup. Backups accumulate one per migrated version and are never deleted by Dito.

## Pitfalls

Moves that look reasonable but go against this decision:

- Patching old shapes inside serde impls, `#[serde(default)]` tricks or `Value` round-trips.
- Renumbering or editing a migration that has shipped; add a new one instead.
- Reading names, templates or defaults from live code inside a migration. Spell them out, so the step keeps its result when a label or a bundled template changes.
- Silently resetting an unreadable file, or loading a newer file and saving it back.
- Changing a persisted shape without bumping the schema version.

## Review Triggers

A second persisted format needs a shared migration runner, or users need to move between builds in both directions.

## Code

`crates/dito-core/src/config/migrate.rs`, `crates/dito-core/src/config/store.rs`, `crates/dito-core/src/store/schema.rs`.
