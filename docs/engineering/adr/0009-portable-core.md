# ADR-0009: The core is independent of UI and macOS; one actor owns the session

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Hotkeys, the HUD, the menu bar, timers and late results all act on one dictation. Mixing that logic into views causes races, and tying it to AppKit blocks testing and any future front end.

## Decision

`dito-core` holds the session, pipeline, providers, config and store, and depends on no GPUI or macOS crate. macOS enters through traits (`Recorder`, `Sink`, `Frontmost`, `FocusWarmer`, `MusicMute`, `SecretStore`). One controller actor owns the session; transitions carry an epoch so late results are ignored; the UI only observes `HudState`.

## Alternatives Considered

### Session logic in the app

- Pros: Fewer layers
- Cons: Races and untestable code
- Why not chosen: Rejected

## Consequences

New behavior is added to the core and tested with fakes; the app only renders and forwards intent. A port to another platform means new trait implementations, not a rewrite.

## Pitfalls

Moves that look reasonable but go against this decision:

- Putting session logic or state in UI code.
- Importing GPUI or AppKit types into `dito-core`.
- Dropping epoch checks.

## Review Triggers

A second front end (another OS, a CLI) is planned.

## Code

`crates/dito-core/src/controller.rs`, `crates/dito-app/src/state.rs`, `docs/engineering/ARCHITECTURE.md`.
