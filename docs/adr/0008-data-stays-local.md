# ADR-0008: User data and secrets stay on the Mac

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Dictations contain whatever people say at work. The only acceptable place for audio and text to leave the Mac is the provider the user chose.

## Decision

- API keys live only in the macOS Keychain, never in config files, logs or copied text.
- History, dictionary, audio and models are stored locally.
- Nothing is sent anywhere except to the providers the user configured; there is no analytics or telemetry.
- Text copied for AI help contains no keys or recordings.

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

Debugging relies on local logs the user chooses to share. Any future sync or telemetry needs explicit consent design and a new ADR.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding key fields to the config.
- Logging full dictations, audio or keys.
- Adding analytics, crash upload or sync without a consent design.

## Review Triggers

Multi-device sync or crash reporting is proposed.

## Code

`crates/dito-core/src/secrets.rs`, `crates/dito-platform/src/system.rs` (`KeychainSecrets`).
