# ADR-0014: Launching Dito opens no window

- **Status**: accepted
- **Date**: 2026-11-09

## Context

Dito is a background tool: the user presses a shortcut and speaks. Starting it, at login or by hand, means the user wants to dictate, not to configure. The settings window used to open on every launch, because the login item never passed `--background`, so each login put a window in front of the user's work. Closing that window left a Dock icon with no way back to it: nothing handled a click on the icon.

## Decision

A launch opens a window by itself in two cases only: Onboarding on first run, and Settings when a permission is missing, because dictation cannot work without it (General for Accessibility, where the banner explains it; Status when only the Microphone is denied). Otherwise Settings open on request: the menu bar item, the Dock menu, a click on the Dock icon, or opening Dito again while it runs (General). `--open-settings` opens it at launch for checking a build.

"Show icon in Dock" (General, on by default) controls the Dock icon. Off, the app runs without one and the icon returns only while a window is open.

## Alternatives Considered

### Open Settings on a manual launch only

- Pros: A person who double-clicks the app sees something
- Cons: One action with two results depending on who started the process; needs a flag the login item must pass
- Why not chosen: The menu bar icon and a second launch already answer "is it running"

### Keep opening Settings at launch

- Pros: Always visible feedback, and the permission banner is seen
- Cons: Interrupts every login and every restart
- Why not chosen: Conflicts with staying out of the way

## Consequences

A revoked permission is noticed at the next launch, not while the app keeps running. The `--background` flag is gone; the dev tasks pass `--open-settings`. Dock-icon changes go through `dock::sync` so the policy follows the windows.

## Pitfalls

Moves that look reasonable but go against this decision:

- Opening Settings at launch "to be helpful", or adding a launch flag the login item must pass.
- Showing a window to report a state that is not blocking dictation.
- Setting the activation policy after launch, or inside a window-close callback.

## Review Triggers

Users report not noticing that Dito is running; a new permission becomes necessary.

## Code

`crates/dito-app/src/startup.rs` (`launch_window`, `reopen`), `crates/dito-app/src/dock.rs`.
