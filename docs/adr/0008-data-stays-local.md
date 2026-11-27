# ADR-0008: User data and secrets stay on the Mac

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Dictations contain whatever people say at work. The only acceptable place for audio and text to leave the Mac is the provider the user chose.

## Decision

- API keys live only in the macOS Keychain, never in config files, logs or copied text.
- History, wordbook, audio and models are stored locally.
- A command run is stored in History with the text it handled: the instruction, the selected or entered text, the result, the router's decision, the tool, the outcome, the time and engine of each step, and any error. It is local, like a dictation, and there is no audio for it.
- Nothing is sent anywhere except to the providers the user configured; there is no analytics or telemetry.
- Starting a local model download fetches a static URL manifest (`GET https://fang2hou.github.io/dito/models/v1.json`) so deployed builds can recover if an upstream model host moves files. The request runs only when the user initiates a download, carries no user data, tokens or telemetry, and falls back silently to the compiled-in URLs when offline or unavailable (2026-11-25).
- While a recording runs, Dito opens the connection to the cloud speech and polish engines in use, so the requests after the recording start sooner. The request that opens it is `HEAD /` to the host of the configured endpoint. It carries no key, no audio and no text, it never follows a redirect to another host, and a press that turns out to be typing does not send it. From it the provider learns only that a recording started. The dictation would tell it the same a moment later, except for a recording that is cancelled or holds no speech, which the provider did not hear of before (2026-11-19).
- The word finder sends the text the user pasted and the text of the file they chose to the text model, and keeps neither: only the words the user then adds are stored. When that model is a cloud model, the dialog names it and the search starts only after the user confirms, each time it opens, that the text is sent there (2026-11-13). Its other AI source sends nothing: the user copies a prompt, gives it and their files to an AI of their choice themselves, and pastes the reply back.
- Chat conversations are stored locally with the files and meetings attached to them (their text is copied in when a message is sent). A conversation goes only to the text model in use, and message text appears in logs only in the `dito_trace` target.
- Live meeting notes, and the questions asked from their panel, go to the chat model in use, like any chat: on this Mac, or to the provider the user chose for a cloud chat model. Meeting paragraphs go to the text model in use for polishing, local or cloud. The notes are held in memory and never stored. A question asked from the panel is an ordinary chat stored locally with the meeting's text copied in (2026-11-27).
- Text copied for AI help contains no keys or recordings.
- The Local API (off by default, bound to 127.0.0.1) is the one inbound path. It lends the models Dito is set to to apps on this Mac; a request that reaches a cloud model is forwarded with the stored key, which the client never sees, and a failure reaches the client as its kind only, never as provider details. Its access token is an owner-only `local_api.token` file in Dito's data directory, and a copied specification shows a placeholder instead of it. Builds without a Team ID are asked for the login Keychain password after every update when they read a Keychain item written by an older build. The token only guards a loopback service whose clients already keep the same token in their own configuration files (2026-11-20).
- Logs never hold keys or audio. Full prompts, instructions and texts appear only in the `dito_trace` debug log, which is off by default and recorded only when the user switches it on: with `RUST_LOG=dito_trace=debug`, or with the Detailed log switch in the Log view of the Status page.
- The Log view shows the app's own log lines since launch from a bounded buffer in memory (the newest 1 000 lines). The buffer is never written to disk or sent anywhere, and it ends with the app. The Detailed log switch is not a saved setting: it is off at every launch and lives for the session, because the detailed log holds full texts. The copy button in the view puts the shown lines on the clipboard, and nothing else leaves the buffer.

## Alternatives Considered

### Keys in the config file

- Pros: Portable
- Cons: Leaks through copies, sync and bug reports
- Why not chosen: Rejected

### Usage telemetry

- Pros: Product insight
- Cons: Breaks the promise; needs consent design
- Why not chosen: Not adopted

## Consequences

Debugging relies on local logs the user chooses to share, or copies from the Log view. Any future sync or telemetry needs explicit consent design and a new ADR.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding key fields to the config.
- Logging full dictations, audio or keys at the default level, or anywhere other than the opt-in `dito_trace` target (never audio or keys, even there).
- Writing the Log view's buffer to disk, sending it anywhere, or saving the Detailed log switch in the config so that full texts are recorded at the next launch.
- Adding analytics, crash upload or sync without a consent design.
- Giving the request that opens a connection ahead of a dictation a key, a body or an API path, or sending it to a host the user did not configure.

## Review Triggers

Multi-device sync or crash reporting is proposed.

## Code

`crates/dito-core/src/secrets.rs`, `crates/dito-core/src/provider/preconnect.rs` (the request that opens a connection ahead of a dictation), `crates/dito-core/src/local_api/` (the loopback service and its token), `crates/dito-platform/src/system/keychain.rs` (`KeychainSecrets`), `crates/dito-core/src/trace.rs` (the debug log target), `crates/dito-app/src/logbook.rs` (the in-memory buffer and the switch), `crates/dito-core/src/store/` (what History keeps), `crates/dito-core/src/meeting_notes/` and `crates/dito-app/src/services/meeting_notes.rs` (the notes and their model source).
