# Glossary

One word per concept across code, documentation, and UI copy. When a name in code disagrees with this table, the table wins and the code is renamed. Persisted keys (TOML, SQLite, Keychain items) keep their shipped spelling: renaming them requires a migration ([ADR 0015](adr/0015-versioned-persistence.md)). `messages.toml` keys follow the screen copy they name ([ADR 0011](adr/0011-japanese-first.md)).

The **JA**, **EN**, and **ZH** columns give the canonical UI terms (`—` when a concept is internal and never appears in UI copy).

## Dictation flow

| Term | Meaning | JA | EN | ZH | Code | Avoid |
| --- | --- | --- | --- | --- | --- | --- |
| Dictation | One run from shortcut to inserted text | 音声入力 | Dictation | 语音输入 | `controller` actor, `DictationHandle` | session, job (for the whole run) |
| Clip | The audio of one recording, 16 kHz mono | — | — | — | `AudioClip`, `Recorder` | recording (as a noun for the audio), sample |
| ASR | Speech recognition, as a stage and as a model kind | 音声認識 | Speech recognition | 语音识别 | `Transcriber`, `AsrRequest`, `asr_*` | STT, recognizer (except sherpa-onnx's native `Recognizer`) |
| Transcribe | Turning a clip into its raw transcript (`Output::raw`) | 認識 | Transcribe | 识别 | `Transcriber::transcribe`, `PipelinePhase::Transcribing`, `HudState::Transcribing` | recognize, recognized text |
| Polish | Cleaning up and formatting a transcript with a language model | 文章整形（整形） | Polish | 文本润色（润色） | `Polisher`, `PolishRequest`, `PolishJob` | rewrite, refine, clean up |
| Pipeline | Transcribe, then polish, for one clip; excludes recording and insertion | — | — | — | `pipeline::run`, `PipelineJob`, `PipelinePhase` | |
| Insertion | Putting the final text into the focused app by key input or paste | 入力 | Input | 输入 | `Sink`, `InsertMethod`, `Delivery` | typing-in, output |
| Delivery | The outcome of an insertion: inserted, copied to the clipboard, or failed | — | — | — | `Delivery` | |
| Common rules | The base polish instructions shared across every app for one language | 共通ルール | Common rules | 通用规则 | `PromptSet::base` | base prompt, global style |
| App rules | Polish instructions and optional writing examples layered on top of the common rules for one app target | アプリ別のルール | App rules | 应用规则 | `AppRules`, `BuiltinAppRules`, `PromptSet::app_rules` | style, app style, profile, preset, app settings |
| App target | Which app rules apply: fallback (`Other`), built in (`Slack`, `Teams`, `Herdr`), or a user-configured app | 対象のアプリ（その他） | Application (Other) | 目标应用（其他） | `AppTarget` | style, profile (persisted SQLite `style` column and `disabled_builtin_styles` TOML key keep their shipped spelling) |
| Wordbook | Words the user wants spelled as written, passed to ASR as hotwords and optionally to polish | 単語帳 | Wordbook | 单词本 | `Store::words`, `PipelineJob::wordbook`, `wordbook_for_polish` | dictionary, glossary (this file), vocabulary (except the `reference_vocabulary` prompt tag) |
| Word finder | Offers terms for the wordbook from pasted text, a chosen file, or the reply of another AI to a copied prompt | 解析して追加 | Analyze and add | 解析并添加 | `word_finder::find_words`, `read_source`, `read_reply`, `Candidate` | auto learning, extraction, import (that adds a word list as it is) |
| Hotword | A wordbook word as ASR receives it | ホットワード | Hotword | 热词 | `AsrRequest::hotwords`, `is_hotword_echo` | vocabulary, keyword |
| Speech language | The spoken language passed to ASR and used to pick polish rules (`ja`, `en`, `zh`, or `Auto`), named as the language as a whole rather than a script or regional variant | 優先言語（日本語, 英語, 中国語; HUD: 日, En, 中） | Preferred language (Japanese, English, Chinese; HUD: 日, En, 中) | 优先语言（日语, 英语, 中文; HUD: 日, En, 中） | `PreferredLanguage`, `Lang::name` | script or regional variants in speech/HUD/prompt pickers |
| History | Stored dictation runs with their audio, and command runs with their instruction, decision, tool, source text, result, outcome, and step times | 履歴 | History | 历史 | `HistoryEntry`, `HistoryKind`, `CommandDetails` | log (the debug log is `dito_trace`, see [DEBUGGING.md](DEBUGGING.md)) |

## Engines and models

| Term | Meaning | JA | EN | ZH | Code | Avoid |
| --- | --- | --- | --- | --- | --- | --- |
| Source | Where a stage runs: on the Mac (`Local`) or over HTTP (`Provider`) | ローカル, クラウド | Local, Cloud | 本地, 云端 | `config::Source` | mode, backend, "provider" in UI copy |
| Engine | The active implementation built from the settings for one stage | エンジン | Engine | 引擎 | `Engines`, `EngineFactory`, `EngineConfig` | |
| Provider | A cloud service that serves a stage | — | — | — | `provider/`, `ProviderError`, `ProviderResult` | vendor, remote |
| Configuration | One saved cloud setup (name, request or endpoint, model), selected from a row of tiles | 設定 | Configuration | 配置 | `CloudAsrConfiguration`, `CloudPolishConfiguration` | profile, preset, connection |
| Template | A bundled starting point for a cloud configuration | テンプレート | Template | 模板 | `CloudAsrTemplate`, `CloudPolishTemplate`, `design/asr-templates/` | preset |
| Profile | An AWS shared-config profile | AWS プロファイル | AWS profile | AWS 配置文件 | `aws_profiles` | any other use (`config/migrate.rs` retains the retired `cloud_asr.profiles` key) |
| Model | Downloadable local weights or a remote model identifier | モデル | Model | 模型 | `ModelSpec`, `ModelState`, `ModelManager` | |
| Runtime | A native inference library behind `dito-local` | — | — | — | `Runtime`, `AsrRuntime`, `LlmRuntime`, `dito-runtimes` | engine, backend |
| Backend | The trait through which `dito-core` reaches local engines | — | — | — | `LocalBackend` | |
| Settings | Everything the user can configure, stored in `config.toml` | 設定 | Settings | 设置 | `Config`, `ConfigStore` | preferences, options |

## Command routing

How a command is decided is in [INTENT_ROUTING.md](INTENT_ROUTING.md); how it is shown is in `design:hud/command`.

| Term | Meaning | JA | EN | ZH | Code | Avoid |
| --- | --- | --- | --- | --- | --- | --- |
| Command | One run from the command shortcut to a replaced selection, an answer, or a cancel | コマンド | Command | 命令 | `RunKind::Command`, `controller` actor | agent, action |
| Selection | The text selected in the frontmost app, captured when a command starts and again when recording ends | 選択範囲 | Selection | 所选内容 | `SelectionSource`, `CapturedSelection` | context, target |
| Surrounding text | The rest of the focused field's text around the selection, trimmed to a budget and passed to tools as reference only | — | — | — | `Surrounding` | selection, context |
| Instruction | What the user asks for, spoken or typed, about the selection | 指示 | Instruction | 指令 | `intent::request` | query, prompt (the prompt is what the model reads) |
| Tool | One prompt for one intent, executed on the selection after routing | — | — | — | `command` module | action, skill |
| Command window | The small window above the capsule when a command needs a choice, text, confirmation, or shows an answer | — | — | — | `HudState::{Choosing, NeedsText, Confirm, Answer}` | panel, dialog, popup |
| Intent | What an instruction asks for: polish, rewrite, translate, summarize, reply, ask, cancel, or none | — | — | — | `Intent` | action, tool |
| Rewrite | Changing length, tone, format, audience, or wording on purpose | 書き換え | Rewrite | 改写 | `Intent::Rewrite`, `RewriteOp` | as a word for the polish stage |
| Router prompt | The fixed prompt listing one lettered option per intent | — | — | — | `intent::system_prompt`, `router_system.txt` | |
| Option | One lettered choice in the router prompt | — | — | — | `intent::OPTIONS` | for anything in settings |
| Reading | What one prompt pass returns without generating tokens | — | — | — | `Reading`, `LlmRuntime::read` | |
| Head | A small fitted classifier over a reading, tied to one model file and one router prompt | — | — | — | `Head`, `Heads` | adapter, layer |
| Decision | The chosen intent with its probability | — | — | — | `Decision` | |

## Interface

| Term | Meaning | JA | EN | ZH | Code | Avoid |
| --- | --- | --- | --- | --- | --- | --- |
| Display language | The UI locale of menus, settings, and the HUD, shown as a self-name including script variant (such as Simplified or Traditional Chinese) | 表示言語（日本語, English, 简体中文） | Display language (日本語, English, 简体中文) | 界面语言（日本語, English, 简体中文） | `DisplayLanguage`, `Lang::native_name` | conflating UI script variants with speech language |
| HUD | The floating capsule that shows live dictation and command state | HUD | HUD | HUD | `hud/`, `HudState`, `HudView` | overlay, toast |
| HUD state | For dictation: hidden, listening, transcribing, polishing, inserted, copied, error; command adds thinking, working, its windows, and replaced | — | — | — | `HudState` | |
| Tile | The horizontal card control representing one saved cloud configuration | — | — | — | `ConfigTile`, `config_tiles/` | card (cards are grouped containers) |
| Page | One screen of the settings window | — | — | — | `settings/*` | tab |
| Motion scope | The owner of animation state for one view | — | — | — | `MotionScope` | |
| Design section | A board or item of `design/dito.figcraft`, referred to by id | — | — | — | `design:<id>`, `design/sections.toml` | page or board numbers |

## Design references

Code, docs, and skills name a design section as `design:<id>` (for example `design:settings/cloud-models/tile-row`), never by number. `design/sections.toml` maps each id to its board and layer name, so renumbering the design file means editing that file and `design/README.md`. `design/README.md` is the one exception: it documents the file's own numbering and cites layers by the names they carry. `mise run check:design-refs` checks that every reference resolves and that the layer names still exist in the saved design.

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
