# Cloud ASR Connection Config (v1)

The contract behind the cloud speech presets: what the JSON config means, how the editor behaves, and how it is stored. `dito-core` implements the request, language mapping, validation and storage; `dito-app` implements the editor and help. The decision itself is [ADR 0007](../docs/adr/0007-declarative-cloud-asr.md).

## UI and Config

- Settings keep three presets (FunASR, Qwen ASR, OpenRouter), each with a base URL, a model ID and an API key.
- The model ID is free text with no suggestions or default; the templates leave it empty, so a preset takes effect only after the user fills it in from the vendor's docs.
- There is no language field on the speech page. The language comes from the global speech language (General), mapped through `language_map`; it never follows the UI language or the macOS language, and it isn't repeated in each profile.
- 接続設定 → 編集 opens a single-column JSON editor about 900 px wide: template name, language-code shortcuts, the editor, a separate live validation area, and Save. There is no request preview and no protocol text under the template name. JSON gets light syntax highlighting; error lines are marked and details appear below the editor. No code completion.
- The simple form and the JSON share one config: the form edits `connection`, the editor edits the whole draft. A draft replaces the active config only after it validates and is saved.
- Each preset keeps its own config. Editing the language map, the request, the response, or the global speech language invalidates earlier connection test results.
- The JSON never holds a real API key. The key comes from its own field and replaces `{{api_key}}` at request time; copied templates and the AI help never include keys or recordings.

## Files

- [JSON Schema](asr-config.schema.json) (documentation only; validation in code is hand-written)
- Templates, compiled into the app: [FunASR](asr-templates/funasr.json), [Qwen ASR](asr-templates/qwen-asr.json), [OpenRouter](asr-templates/openrouter.json)

The schema covers static structure. Whether variables exist, URLs join correctly or language keys collide is checked semantically. A config that passes validation isn't proof that the vendor accepts it.

## Top Level

| Field | Meaning |
| --- | --- |
| `version` | Config version, currently `1` |
| `connection` | Base URL and model the user edits |
| `language_map` | Standard language codes to the vendor's values; `other` is required |
| `request` | Method, path, query, headers, encoding and body |
| `response` | Where to read the transcript and, optionally, the detected language |

## Language Map

The speech language produces `ja`, `en`, `zh`, or nothing (automatic). Automatic maps through the reserved key `auto`; the string "auto" is never sent unless configured. `other` is the fallback, not a language. Region tags such as `ja-JP` are accepted as keys and values, but the setting itself never produces them.

- Keys are standard language tags (`ja`, `en`, `zh`, or region tags such as `zh-CN`). `JP` is a country code; the key for Japanese is `ja` or `ja-JP`.
- Values follow the vendor and keep their case, so `ja` can map to `ja`, `JA`, `JP` or `Japanese`.
- Matching order: full tag, then primary language, then `other`. Keys are compared after BCP 47 case normalization; keys that collide after normalization are an error. Values are never normalized.
- `other` must be a non-empty string and is formatted last. Any other value may be `null`, meaning "don't specify". `"auto": null` keeps automatic recognition and sends no language field. When the setting is automatic, `other` is not used: an unset `auto` also omits the field, and a string `auto` is sent as is.

Standard codes:

```json
{ "ja": "ja", "en": "en", "zh": "zh", "auto": null, "other": "en" }
```

Region codes, only for vendors that need them:

```json
{ "ja": "ja-JP", "en": "en-US", "zh": "zh-CN", "auto": null, "other": "en-US" }
```

| Setting | Relevant entry | Sent |
| --- | --- | --- |
| `ja` | `ja: "JP"` | `JP` |
| `ja-JP` | no region entry, `ja: "ja"` | `ja` |
| `fr` | no `fr`, `other: "en"` | `en` |
| automatic | `auto: null` | nothing |

The map only produces a value. Where it goes is decided by `{{language}}` in the request (body, query or header); a template that doesn't reference it never sends a language field.

## Request

`connection.base_url` is the API base, including any relay prefix, without the final path, query or fragment. `request.path` starts with `/` and is appended; only the duplicate slash at the joint is removed. For example, `https://relay.example.com/vendor/v1` plus `/chat/completions` gives `https://relay.example.com/vendor/v1/chat/completions`. A port goes in the base URL; there is no port field.

| Field | Rule |
| --- | --- |
| `method` | `POST` or `PUT` (all presets use `POST`) |
| `path` | The appended path; the query is separate, never a full URL |
| `query` | Query parameters, may use variables |
| `headers` | Headers, for example `Authorization: "Bearer {{api_key}}"` |
| `encoding` | `json` or `multipart` |
| `body` | JSON body with nested objects and arrays; `json` only |
| `fields` | Multipart fields; `multipart` only; the file field is `{{audio.file}}` |

The presets use each vendor's synchronous HTTP API. v1 doesn't describe WebSocket sessions, polling or multi-step uploads; those need a dedicated adapter and a new version.

## Variables

| Variable | Value |
| --- | --- |
| `{{model}}` | `connection.model` |
| `{{language}}` | The speech language after `language_map` |
| `{{api_key}}` | The key saved for this preset |
| `{{audio.base64}}` | The WAV as plain Base64, without a `data:` prefix |
| `{{audio.data_uri}}` | The WAV with a `data:audio/wav;base64,` prefix |
| `{{audio.file}}` | The WAV as a multipart file; only as the whole value of a multipart field |

Variables are fixed placeholders; nothing is evaluated. Unknown variables are an error before saving.

- A value that is exactly `"{{language}}"` and maps to `null` removes its field; the string "null" is never sent.
- `{{language}}` inside a longer string that maps to `null` is an error, and so is a removable language variable directly inside an array.
- A `null` the user writes stays `null`.
- JSON requests set Content-Type automatically; multipart generates it with the boundary. Query values are URL-encoded and JSON is serialized, never concatenated.

## Response

`response.text_pointer` is a JSON Pointer (RFC 6901), for example `/text`, `/output/text` or `/choices/0/message/content`; escape `~` and `/` as `~0` and `~1`. The text is trimmed. A missing path or a non-string value is a response format error; an empty string means no speech. `language_pointer` is optional and never overrides the speech language setting.

## Presets

- **FunASR**: synchronous HTTP, Data URI audio, text at `/output/text`. No language field, so the service detects it. Based on the vendor's synchronous HTTP docs.
- **Qwen ASR**: an audio message body, language at `asr_options.language`, text at `/choices/0/message/content`. Based on the Qwen ASR API reference.
- **OpenRouter**: a transcription body with Base64 audio and format as separate fields, language at top-level `language`, text at `/text`. Based on OpenRouter's speech-to-text guide.

The templates use the vendors' public endpoints; users can switch to a workspace endpoint or a relay. They are references and haven't been verified against a real account.

## Editor and Validation

1. Opening 接続設定 loads the current preset. Each preset keeps its own URL, model, key and config; templates fill in only on first use. Switching presets keeps each one's unsaved edits; Save applies to the current preset; Cancel changes nothing. Only an explicit template reload that would overwrite edits asks for confirmation.
2. Two language-map shortcuts fill `ja`, `en`, `zh` and `other`, keeping extra languages and the `auto` choice; they do nothing while the JSON doesn't parse. The editor offers 設定をコピー and the help page offers AI 用の説明をコピー.
3. Validation runs as you type: syntax, schema, `other`, placeholders, response pointer, URL, encoding and audio variables. It is debounced (about 200 ms); any edit invalidates the previous result at once. While checking, it shows 確認しています… and Save is disabled. Errors show the line, column and field path; Save is enabled only when everything passes.
4. The editor shows only JSON and error markers, with a thin divider above the line count. Below it, a separate area shows 入力内容の確認 and the result. Keys and audio variables are never expanded; the connection test stays on the Models page.
5. Cancel keeps the active config; the last valid config is kept for recovery.

Typical errors: missing `language_map.other`; undefined placeholder `{{lang}}`; an unparseable URL; `json` and `multipart` fields mixed; an invalid response pointer; `{{audio.file}}` in a JSON body.

Japanese copy uses short, familiar settings words (接続設定, 言語コード, 入力内容の確認, ヘルプ, AI 用の説明をコピー) and polite sentences ending in です and ます. JSON keys, variables and API names stay as they are.

## Copy for AI

AI 用の説明をコピー copies the config reference in the current UI language: field meanings, the language map, request and response formats, variables, constraints and the selected template. It contains no product design notes, task instructions or prompts, and never keys or recordings. The content is wrapped in:

```text
<dito_speech_model_config_spec>
reference and template
</dito_speech_model_config_spec>
```

## Help Page

- Same window as the editor: up to 900 px wide, height following content and the screen.
- A row of four equal blocks shows the order template, edit, automatic check, save. Each has 12 pt side padding, a 32 pt number slot and 12 pt spacing; numbers sit in thin gold circles (Outfit 14 SemiBold, tabular) above 13 Medium step text, joined by fixed-width arrows.
- Four cards follow, each under a small gold marker without numbers: config structure, language mapping, variables, request and transcript.
- The page closes only with × at the top right and returns to the editor. The footer holds only AI 用の説明をコピー; copying the current config lives in the editor.
- Styling comes from the design file's `10 Color` and `11 Type, Space & Components`: title 26 Bold at 160%, group titles 17 Bold, body 14 Regular, labels 13 Medium, captions 12 Regular; window padding 28, card padding 24, group spacing 24, card radius 16, window radius 20. Colors use the existing `bg`, `card`, `sub`, `border`, `text`, `muted`, `accentSoft`, `primary` and `onPrimary` tokens; the light primary button is Plain Black and gold stays small.
- The ヘルプ button uses the light accent style: Gold 01 fill, Gold 04 border and accent text in light; `accentSoft` fill with Gold 04 text in dark.

The editor stays plain: the title is just 接続設定, with no template instructions, validation explainer, fake file name or shortcut labels. The full rules live on 設定の書き方 only.

## Consistency and Connection State

- The main settings and onboarding use the same names: ベース URL, モデル ID, API キー, 接続設定, 接続テスト. The settings page shows fields and actions only.
- 接続テスト is enabled only with a valid URL, a model ID, a usable config and, where needed, a key. Onboarding's 続ける stays disabled until a test succeeds. Tests can't be triggered twice at once, and any change to URL, model, key, request, response or language rules invalidates the result.
- Success in onboarding shows 接続済み, never a latency number. Failures show a short Japanese message about the actual problem.
- The editor and help share a compact close icon labeled 閉じる for accessibility. It is a 32 × 32 ghost button with a 12 × 12 vector cross: no fill or border at rest, a round `sub` fill on hover (120 ms), and it closes only the current layer.

## Window and Scrolling

- The dialog is `min(900, available width − 48)` wide and `min(content height, available height − 48)` tall, with at least 24 pt above and below. The help page's natural height is about 836 pt and the editor's about 900 pt; these are design references, not fixed heights. The design shows a 620 pt short window for the help page (top, middle, bottom) and the editor (middle), in light and dark.
- The help page has a fixed header, a shrinkable scroll area (`min-height: 0`) and a fixed footer; the page itself never scrolls.
- Scroll areas fade 24 pt at the top and bottom edges (about 94% background to transparent), following the theme and ignoring the pointer. The top fade shows once scrolled; the bottom fade shows while more content remains.
- In the editor, only the JSON scrolls. Title, close, preset, language map, copy, validation area and footer stay fixed; a taller window only grows the code viewport (`flex: 1`, `min-height: 0`), so there is no nested scroll. The line count and its divider stay at the bottom of the code area.
- A thin scrollbar (4 pt, visible track and thumb, 8 pt from the edge) appears only on overflow; the thumb shows the visible share and position, and wheel, trackpad and dragging all scroll. The hit area is wider than the line and hover makes it more visible without shifting the code. The code area fades 20 pt at its edges without covering the scrollbar, line count or validation area.

## Storage

Settings are TOML. `cloud_asr.profiles` is a list of presets, each with a stable `id`, a display `name` and its `config`; `cloud_asr.selected` holds the id of the active one. Each `config` is stored as a JSON string and parsed back on load, which keeps `null`, nested arrays and custom fields exactly. Keys are stored separately in the Keychain, under an account named after the preset's id, and never in this JSON.
