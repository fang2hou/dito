# Glossary

One word per concept, in code, docs and conversation. When a name in the code disagrees with this table, the table wins and the code is renamed. Persisted keys (TOML, SQLite, Keychain items) and `messages.toml` keys keep their shipped spelling: renaming them needs a migration ([ADR 0015](adr/0015-versioned-persistence.md)).

## Dictation flow

| Term | Meaning | Code | Avoid |
| --- | --- | --- | --- |
| Dictation | One run from shortcut to inserted text | `controller` actor, `DictationHandle` | session, job (for the whole run) |
| Clip | The audio of one recording, 16 kHz mono | `AudioClip`, `Recorder` | recording (as a noun for the audio), sample |
| ASR | Speech recognition as a stage and a model kind | `Transcriber`, `AsrRequest`, `asr_*` | STT, recognizer (except sherpa-onnx's native `Recognizer`) |
| Transcribe | The verb for the ASR stage; its result is the transcript (`Output::raw`) | `Transcriber::transcribe`, `PipelinePhase::Transcribing`, `HudState::Transcribing` | recognize, recognition |
| Polish | Rewriting the transcript with a language model; also the stage's name | `Polisher`, `PolishRequest`, `PolishJob` | rewrite, refine, clean up |
| Pipeline | Transcribe, then polish, for one clip; no recording and no insertion | `pipeline::run`, `PipelineJob`, `PipelinePhase` | |
| Insertion | Putting the final text into the focused app, by typing or paste | `Sink`, `InsertMethod`, `Delivery` | typing-in, output |
| Delivery | The result of an insertion: inserted, copied to the clipboard, or failed | `Delivery` | |
| Style | The writing profile that shapes a polish prompt; built in (Slack, Teams, Herdr) or per app | `Style`, `BuiltinStyle`, `StylePrompt` | profile, preset |
| Dictionary | Words the user wants recognized, passed as hotwords and optionally to polish | `store` dictionary, `dictionary` field | glossary (that is this file), vocabulary |
| History | Stored dictation results with their audio | `HistoryEntry`, `HistoryKind` | log |

## Engines and models

| Term | Meaning | Code | Avoid |
| --- | --- | --- | --- |
| Source | Where a stage runs: `Local` (on the Mac) or `Provider` (cloud) | `config::Source` | mode, backend |
| Engine | The implementation chosen for one stage, built from the settings | `Engines`, `EngineFactory`, `EngineConfig` | |
| Provider | A cloud service that serves a stage | `provider/`, `ProviderError`, `ProviderResult` | vendor, remote |
| Configuration | One saved cloud setup (name, request, model), selected from a row of tiles | `CloudAsrConfiguration`, `CloudPolishConfiguration` | profile, preset, connection |
| Template | A bundled starting point for a configuration | `CloudAsrTemplate`, `CloudPolishTemplate`, `design/asr-templates/` | preset |
| Profile | Only an AWS shared-config profile | `aws_profiles` | any other use |
| Model | Downloadable local weights | `ModelSpec`, `ModelState`, `ModelManager` | |
| Runtime | A native inference library behind `dito-local` | `Runtime`, `AsrRuntime`, `LlmRuntime`, `dito-runtimes` | engine, backend |
| Backend | Only the trait through which core reaches local engines | `LocalBackend` | |
| Settings | Everything the user can change, stored in `config.toml` | `Config`, `ConfigStore` | preferences, options |

## Interface

| Term | Meaning | Code | Avoid |
| --- | --- | --- | --- |
| HUD | The floating capsule that shows progress | `hud/`, `HudState`, `HudView` | overlay, toast |
| HUD state | One of hidden, listening, transcribing, polishing, inserted, copied, error | `HudState` | |
| Tile | The UI control for one configuration; a UI word, never used in core | `ConfigTile`, `config_tiles/` | card (cards are the grouped containers) |
| Page | One screen of the settings window | `settings/*` | tab |
| Motion scope | The owner of animation state for one view | `MotionScope` | |
| Design section | A board of `design/dito.figcraft`, referred to by id | `design:<id>`, `design/sections.toml` | page or board numbers |

## Design references

Code, docs and skills name a design section as `design:<id>` (for example `design:models/cloud/tile-row`), never by number. `design/sections.toml` maps each id to its board and layer name, so renumbering the design file means editing one file. `mise run check:design-refs` checks that every reference resolves and that the layer names still exist in the saved design.

## Where each concept lives

| Concept | Crate | ADR |
| --- | --- | --- |
| Dictation flow, pipeline, controller | `dito-core` | [3](adr/0003-never-lose-speech.md), [5](adr/0005-reliable-delivery.md) |
| Cloud ASR requests | `dito-core` (`provider/configured_asr`) | [7](adr/0007-declarative-cloud-asr.md) |
| Local models and runtimes | `dito-local`, `dito-runtimes` | [2](adr/0002-local-first.md) |
| Microphone | `dito-audio` | |
| Shortcuts, permissions, insertion, Keychain | `dito-platform` | [6](adr/0006-minimal-permissions.md) |
| Settings, schema versions | `dito-core` (`config`) | [10](adr/0010-one-home-per-setting.md), [15](adr/0015-versioned-persistence.md) |
| History, dictionary, stats | `dito-core` (`store`) | [8](adr/0008-data-stays-local.md) |
| UI copy | `dito-core` (`i18n/messages.toml`) | [11](adr/0011-japanese-first.md) |
| HUD, settings, onboarding | `dito-app` | [4](adr/0004-hud-out-of-the-way.md), [13](adr/0013-ui-state-ownership.md), [14](adr/0014-silent-launch.md) |
