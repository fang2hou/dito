# ADR-0013: UI state belongs to its view and never blocks rendering

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Global animation maps replayed old animations and leaked; synchronous queries in render froze the settings window. Both came from state with no clear owner.

## Decision

Each window, page and onboarding step owns its motion state (`MotionScope`, reached through weak handles and reset when the view leaves). Data for pages loads in the background into snapshots with request tickets; stale results are dropped and failures keep the last data. Nothing slow runs in render. A polling loop owned by one view is held as a `Task` in that view, so it ends with the view or when the view replaces it.

Two process-wide settings are the only allowed exceptions to "no global or thread-local UI state". The ambient `MotionScope` handle (`AMBIENT` in `ui/scope.rs`) is thread-local, installed by `with_motion_scope` only for the duration of one render call and restored by a drop guard even on unwind, so it never outlives the render or leaks into another view's scope. The UI language (`set_ui_language` in `ui/typography.rs`) is one value for the whole app, applied at the start of a window's render so the CJK typeface follows it; no view holds a different language, so there is no per-view state to confuse.

Each window owns one shared hover-tooltip layer. Trigger leases belong to their element trees and retain weak handles to their view scopes. Rapid handoffs reuse the visible bubble, retargeting position and width from their current values and velocities. Text stays single-line, centered, and softly clipped at both horizontal edges. Scope resets, scroll, Escape, and removing a trigger dismiss stale tips. Transition frames request redraws only while moving or waiting to dismiss.

The tooltip layer retains only its last selected anchor through the exit fade, even when its trigger unmounts. That anchor can paint the outgoing bubble but cannot request a tooltip in a later frame. Scroll and Escape suppress a still-hovered trigger until it is left, and fade from the current opacity rather than discarding the visual. HUD hover disclosures likewise stay mounted through exit and keep their settled layout anchor; the non-activating panel's pointer poll owns the combined capsule, gap and Stop hover zone.

The dictation controller owns a rolling in-memory timing estimate. The HUD owns the displayed progress curve and retargets it from its current position and velocity when results arrive. Text delivery happens immediately, while the HUD waits for its final fill before showing the outcome.

## Alternatives Considered

### Global registries and inline queries

- Pros: Less code
- Cons: Replays, leaks and frozen UI
- Why not chosen: Replaced

## Consequences

New views follow the same pattern: own your state, load in the background, apply only current results.

The inline Cloud ASR editor keeps one draft per configuration, by configuration id, never by position: an editor entity with the request JSON and its native undo history, the key as typed, the validation state and the connection-test result. A draft starts the first time its configuration is shown, survives selecting, reordering and opening or closing documentation, and is dropped when the configuration is deleted. A revision-guarded 200 ms validation task per draft saves only valid drafts, into that draft's own configuration and never changing the selection; invalid edits retain the last valid saved configuration. Connection-test results are accepted only for the same draft revision and preferred language. The settings shell presents the editor-owned help overlay as the sole dialog. Its header and footer stay fixed while only the four-card documentation body scrolls.

The Prompt page owns grouped history per editor, language and application. Committed input updates drafts, idle tasks are revision guarded, and IME marked text is excluded. Toolbar and native editing actions use the same grouped history. Draft autosave waits for 3 s idle or an actual editor exit. An untouched editor cannot flush another editor's active draft. Autosave failures retain drafts and expose a retryable unsaved state; scoped resets require confirmation and remain undoable.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding global or thread-local UI state.
- Calling the store or the network from render.
- Showing an empty list when a load fails.

## Review Triggers

GPUI provides element-owned animation state or async data primitives.

## Code

`crates/dito-app/src/ui/scope.rs`, `crates/dito-app/src/settings/query_state.rs`, `crates/dito-app/src/settings/page_query.rs`, `crates/dito-app/src/services/`.
