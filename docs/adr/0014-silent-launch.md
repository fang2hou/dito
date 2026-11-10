# ADR-0014: Launching Dito opens no window

- **Status**: accepted
- **Date**: 2026-11-09

## Context

Dito is a background tool: the user presses a shortcut and speaks. Starting it, at login or by hand, means the user wants to dictate, not to configure. The settings window used to open on every launch, because the login item never passed `--background`, so each login put a window in front of the user's work. Closing that window left a Dock icon with no way back to it: nothing handled a click on the icon.

## Decision

A launch opens a window by itself in three cases only: Onboarding on first run; Settings when a permission is missing, because dictation cannot work without it (General for Accessibility, where the banner explains it; Status when only the Microphone is denied); and the startup failure window (design 20.10) when `config.toml` cannot be read, or `config.toml` or `dito.db` was saved by a newer Dito, because nothing can start without them. Otherwise Settings open on request: the menu bar item, the Dock menu, a click on the Dock icon, or opening Dito again while it runs (General). `--open-settings` opens it at launch for checking a build.

The third window exists because the alternative is worse than a window: the process used to exit with no UI, so a login item died unseen and left a Dito that was silently not running. Like the other two it blocks dictation, so showing it does not break the rule against reporting states that do not. It starts nothing else (no hotkeys, HUD, menu bar item or dictation), follows the system appearance and language because the saved ones cannot be read, and never changes a file by itself. The person ends it with Quit or, for unreadable settings only, with the restore button, which keeps the file as a backup beside it, writes the default settings and carries on into Onboarding in the same process (ADR-0015 forbids a silent reset, not one the person asks for). Data from a newer Dito offers no reset: resetting would hide it, so the only fix on offer is updating Dito.

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
- Opening the startup failure window for an error that leaves dictation working, or letting it change a file without the person pressing the restore button.

## Review Triggers

Users report not noticing that Dito is running; a new permission becomes necessary.

## Code

`crates/dito-app/src/startup.rs` (`launch_window`, `reopen`), `crates/dito-app/src/dock.rs`, `crates/dito-app/src/startup_failure/` (the third window), `crates/dito-core/src/config/restore.rs` (the restore).
