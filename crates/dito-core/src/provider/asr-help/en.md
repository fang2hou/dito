# Dito speech model configuration specification v1

The JSON format and constraints accepted by Dito for speech recognition connections. The attached template is a reference example. Endpoint URLs, authentication, and request and response formats depend on the service API. The model ID is a required user-supplied string, with no fixed choices or default.

## Structure

- `version`: `1`.
- `connection`: `base_url` and `model`. The base URL must use HTTP or HTTPS and may include a relay path prefix and port. Exclude the final request path, query, and fragment.
- `language_map`: maps Dito language codes to the values required by the service.
- `request`: `method`, `path`, `headers`, `encoding`, and optionally `query`, `body`, or `fields`.
- `response`: `text_pointer` and an optional `language_pointer`.

## Language mapping

The language comes from the global preferred speech language in General settings, independently of the interface language. There is no separate language selector in this configuration.

### Codes currently supplied by Dito

| Global preferred speech language | Internal value | `language_map` key |
| --- | --- | --- |
| Japanese | `ja` | `ja` |
| English | `en` | `en` |
| Chinese | `zh` | `zh` |
| Auto | Unspecified (`null`) | Reserved key `auto` |

The current preferred speech language setting supplies only these three language codes or Auto. Auto does not supply the literal string `auto`; it supplies no language, and mapping resolves this state through the reserved `auto` key. `other` is a reserved fallback for unknown languages, not a language choice.

`ja-JP`, `en-US`, `zh-CN`, `JP`, `EN`, and `Japanese` are not internal codes emitted by the current setting. When a service requires one of these values, it belongs on the right side of the mapping, for example `"ja": "JP"`. The format also accepts region-tagged input keys, but the current setting does not generate region-tagged codes. Chinese uses the internal code `zh`, not `cn`.

Keys are language tags such as `ja`, `en`, `zh`, or `ja-JP`. `JP` is a country code; the input key for Japanese is `ja` or `ja-JP`. Values must match the service's requirements, for example `ja`, `JA`, `JP`, or `Japanese`. Output values retain their case.

Match the full language tag first, then the primary language, then `other`. Keys match case-insensitively and must not conflict. `other` is required, is a non-empty string, and is placed last. It handles unknown languages. Other mapping values may be `null`.

The reserved `auto` state is distinct from an unknown language. An absent `auto` entry or `"auto": null` omits the language field. An explicit string sends that value. Services without a language parameter do not need a language field.

## Request

`method` is `POST` or `PUT`. `path` is a relative API path starting with `/`, without a complete URL, query, or fragment. Append it to the base URL without losing the relay prefix. For example, `https://relay.example.com/vendor/v1` plus `/chat/completions` becomes `https://relay.example.com/vendor/v1/chat/completions`.

`encoding: "json"` uses `body`, while `encoding: "multipart"` uses `fields`; these are mutually exclusive. JSON bodies support nested objects and arrays. Values in `query` and `fields` may be strings, numbers, booleans, or `null`; header values must be strings. Dito sets Content-Type and the multipart boundary, encodes query parameters, and escapes JSON strings.

This format supports synchronous HTTP only, not WebSocket sessions, task polling, or multi-step uploads.

## Variables

- `{{model}}`: `connection.model`.
- `{{language}}`: the mapped language value.
- `{{api_key}}`: the current platform's API key.
- `{{audio.base64}}`: raw WAV audio encoded as Base64, without a prefix.
- `{{audio.data_uri}}`: audio including the `data:audio/wav;base64,` prefix.
- `{{audio.file}}`: binary WAV audio for multipart; use only as an entire value in `fields`.

Real API keys and recordings are not stored in this configuration. Authentication uses `{{api_key}}`. Variables are fixed substitutions, not executable expressions or scripts. Unknown variables are errors.

When an entire object-field value is `"{{language}}"` and the mapping returns `null`, omit that field. Its use inside a larger string or as an array element is invalid. Explicit JSON `null` values are preserved.

## Response

`text_pointer` and the optional `language_pointer` use JSON Pointer. Examples: `/text`, `/output/text`, `/choices/0/message/content`. Array indexes are numeric; `~` is escaped as `~0` and `/` as `~1`. The transcript must be a string; its leading and trailing whitespace is trimmed. A returned language does not override Dito's global language setting.

## Selected reference template
