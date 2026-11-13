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
- Text copied for AI help contains no keys or recordings.
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

## Review Triggers

Multi-device sync or crash reporting is proposed.

## Code

`crates/dito-core/src/secrets.rs`, `crates/dito-platform/src/system/keychain.rs` (`KeychainSecrets`), `crates/dito-core/src/trace.rs` (the debug log target), `crates/dito-app/src/logbook.rs` (the in-memory buffer and the switch), `crates/dito-core/src/store/` (what History keeps).
