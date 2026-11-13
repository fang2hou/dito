# Glossary

One word per concept, in code, docs and conversation. When a name in the code disagrees with this table, the table wins and the code is renamed. Persisted keys (TOML, SQLite, Keychain items) keep their shipped spelling: renaming them needs a migration ([ADR 0015](adr/0015-versioned-persistence.md)). `messages.toml` keys follow the screen copy they name, not this table ([ADR 0011](adr/0011-japanese-first.md)).

## Dictation flow

| Term | Meaning | Code | Avoid |
| --- | --- | --- | --- |
| Dictation | One run from shortcut to inserted text | `controller` actor, `DictationHandle` | session, job (for the whole run) |
| Clip | The audio of one recording, 16 kHz mono | `AudioClip`, `Recorder` | recording (as a noun for the audio), sample |
| ASR | Speech recognition, as a stage and as a model kind; "speech recognition" is its spelled-out name | `Transcriber`, `AsrRequest`, `asr_*` | STT, recognizer (except sherpa-onnx's native `Recognizer`) |
| Transcribe | The verb for the ASR stage; its result is the transcript (`Output::raw`) | `Transcriber::transcribe`, `PipelinePhase::Transcribing`, `HudState::Transcribing` | recognize, recognition (except in "speech recognition"), recognized text |
| Polish | Rewriting the transcript with a language model; also the stage's name | `Polisher`, `PolishRequest`, `PolishJob` | rewrite, refine, clean up |
| Pipeline | Transcribe, then polish, for one clip; no recording and no insertion | `pipeline::run`, `PipelineJob`, `PipelinePhase` | |
| Insertion | Putting the final text into the focused app, by typing or paste | `Sink`, `InsertMethod`, `Delivery` | typing-in, output |
| Delivery | The result of an insertion: inserted, copied to the clipboard, or failed | `Delivery` | |
| Style | The writing rules that shape a polish prompt: General, built in (Slack, Teams, Herdr) or per app | `Style`, `BuiltinStyle`, `StylePrompt` | profile, preset |
| Wordbook | Words the user wants spelled correctly, passed to ASR as hotwords and optionally to polish | `Store::words`, `PipelineJob::wordbook`, `wordbook_for_polish` | dictionary, glossary (that is this file), vocabulary (except the `reference_vocabulary` prompt tag, which the models read) |
| Hotword | A wordbook word as ASR receives it | `AsrRequest::hotwords`, `is_hotword_echo` | vocabulary, keyword |
| History | Stored dictation results with their audio, and one row for each command run with its instruction, decision, tool, source text, result, outcome and step times | `HistoryEntry`, `HistoryKind`, `CommandDetails` | log (the debug log is `dito_trace`, see [DEBUGGING.md](DEBUGGING.md)) |

## Engines and models

| Term | Meaning | Code | Avoid |
| --- | --- | --- | --- |
| Source | Where a stage runs: `Local` (on the Mac) or `Provider` (cloud) | `config::Source` | mode, backend |
| Engine | The implementation chosen for one stage, built from the settings | `Engines`, `EngineFactory`, `EngineConfig` | |
| Provider | A cloud service that serves a stage | `provider/`, `ProviderError`, `ProviderResult` | vendor, remote |
| Configuration | One saved cloud setup (name, request, model), selected from a row of tiles | `CloudAsrConfiguration`, `CloudPolishConfiguration` | profile, preset, connection |
| Template | A bundled starting point for a configuration | `CloudAsrTemplate`, `CloudPolishTemplate`, `design/asr-templates/` | preset |
| Profile | Only an AWS shared-config profile | `aws_profiles` | any other use (`config/migrate.rs` still names the retired `cloud_asr.profiles` key) |
| Model | Downloadable local weights | `ModelSpec`, `ModelState`, `ModelManager` | |
| Runtime | A native inference library behind `dito-local` | `Runtime`, `AsrRuntime`, `LlmRuntime`, `dito-runtimes` | engine, backend |
| Backend | Only the trait through which core reaches local engines | `LocalBackend` | |
| Settings | Everything the user can change, stored in `config.toml` | `Config`, `ConfigStore` | preferences, options |

## Command routing

How a command is decided is in [INTENT_ROUTING.md](INTENT_ROUTING.md); how it is shown is in `design:hud/command`.

| Term | Meaning | Code | Avoid |
| --- | --- | --- | --- |
| Command | One run from the command shortcut to a replaced selection, an answer or a cancel | `RunKind::Command`, `controller` actor | agent, action |
| Selection | The text selected in the app in front, captured when a command starts and again when its recording ends | `SelectionSource`, `CapturedSelection` | context, target |
| Surrounding text | The rest of the focused field's text around the selection, read with the selection and cut to a budget for the tools, as reference only | `Surrounding` | selection, context |
| Instruction | What the user asks for, spoken or typed, about the selection | `intent::request` | query, prompt (the prompt is what the model reads) |
| Tool | What runs after the decision: one prompt for one intent, on the selection | `command` module | action, skill |
| Command window | A small window above the capsule, opened only when the command needs a choice, text or a confirmation, or has an answer | `HudState::{Choosing, NeedsText, Confirm, Answer}` | panel, dialog, popup |
| Intent | What an instruction asks for: polish, rewrite, translate, summarize, reply, ask, cancel or none | `Intent` | action, tool (a tool is what runs after the decision) |
| Rewrite | Only the intent: changing length, tone, format, audience or wording on purpose | `Intent::Rewrite`, `RewriteOp` | as a word for the polish stage |
| Router prompt | The fixed prompt that lists one lettered option per intent | `intent::system_prompt`, `router_system.txt` | |
| Option | One lettered choice of the router prompt | `intent::OPTIONS` | for anything in the settings |
| Reading | What one prompt pass returns with nothing generated | `Reading`, `LlmRuntime::read` | |
| Head | A small fitted classifier over a reading, valid for one model file and one router prompt | `Head`, `Heads` | adapter, layer |
| Decision | The chosen intent with its probability | `Decision` | |

## Interface

| Term | Meaning | Code | Avoid |
| --- | --- | --- | --- |
| HUD | The floating capsule that shows progress | `hud/`, `HudState`, `HudView` | overlay, toast |
| HUD state | For a dictation: hidden, listening, transcribing, polishing, inserted, copied, error. A command adds thinking, working, its windows and replaced | `HudState` | |
| Tile | The UI control for one configuration; a UI word, never used in core | `ConfigTile`, `config_tiles/` | card (cards are the grouped containers) |
| Page | One screen of the settings window | `settings/*` | tab |
| Motion scope | The owner of animation state for one view | `MotionScope` | |
| Design section | A board of `design/dito.figcraft`, referred to by id | `design:<id>`, `design/sections.toml` | page or board numbers |

## Design references

Code, docs and skills name a design section as `design:<id>` (for example `design:settings/cloud-models/tile-row`), never by number. `design/sections.toml` maps each id to its board and layer name, so renumbering the design file means editing that file and `design/README.md`. `design/README.md` is the one exception to the rule: it documents the file's own numbering and cites layers by the names they carry. `mise run check:design-refs` checks that every reference resolves and that the layer names still exist in the saved design.

## Where each concept lives

| Concept | Crate | ADR |
| --- | --- | --- |
| Dictation flow, pipeline, controller | `dito-core` | [3](adr/0003-never-lose-speech.md), [5](adr/0005-reliable-delivery.md) |
| Cloud ASR requests | `dito-core` (`provider/configured_asr`) | [7](adr/0007-declarative-cloud-asr.md) |
| Local models and runtimes | `dito-local`, `dito-runtimes` | [2](adr/0002-local-first.md) |
| Intent routing | `dito-core` (`intent`), read through `dito-local`'s `LlmRuntime` | |
| Microphone | `dito-audio` | |
| Shortcuts, permissions, insertion, Keychain | `dito-platform` | [6](adr/0006-minimal-permissions.md) |
| Settings, schema versions | `dito-core` (`config`) | [10](adr/0010-one-home-per-setting.md), [15](adr/0015-versioned-persistence.md) |
| History, wordbook, stats | `dito-core` (`store`) | [8](adr/0008-data-stays-local.md) |
| UI copy | `dito-core` (`i18n/messages.toml`) | [11](adr/0011-japanese-first.md) |
| HUD, settings, onboarding | `dito-app` | [4](adr/0004-hud-out-of-the-way.md), [13](adr/0013-ui-state-ownership.md), [14](adr/0014-silent-launch.md) |
