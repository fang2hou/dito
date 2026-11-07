# ADR-0013: UI state belongs to its view and never blocks rendering

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Global animation maps replayed old animations and leaked; synchronous queries in render froze the settings window. Both came from state with no clear owner.

## Decision

Each window, page and onboarding step owns its motion state (`MotionScope`, reached through weak handles and reset when the view leaves). Data for pages loads in the background into snapshots with request tickets; stale results are dropped and failures keep the last data. Nothing slow runs in render.

Each window owns one shared hover-tooltip layer. Trigger leases belong to their element trees and retain weak handles to their view scopes. Rapid handoffs reuse the visible bubble, retargeting position and width from their current values and velocities. Text stays single-line, centered, and softly clipped at both horizontal edges. Scope resets, scroll, Escape, and removing a trigger dismiss stale tips. Transition frames request redraws only while moving or waiting to dismiss.

The dictation controller owns a rolling in-memory timing estimate. The HUD owns the displayed progress curve and retargets it from its current position and velocity when results arrive. Text delivery happens immediately, while the HUD waits for its final fill before showing the outcome.

## Alternatives Considered

### Global registries and inline queries

- Pros: Less code
- Cons: Replays, leaks and frozen UI
- Why not chosen: Replaced

## Consequences

New views follow the same pattern: own your state, load in the background, apply only current results.

The inline Cloud ASR editor owns one persistent editor entity per template, including drafts and native undo history. Opening or closing documentation does not remount these editors. A revision-guarded 200 ms validation task saves only valid drafts; invalid edits retain the last valid saved configuration. Connection-test results are accepted only for the same draft revision and preferred language. The settings shell presents the editor-owned help overlay as the sole dialog. Its header and footer stay fixed while only the four-card documentation body scrolls.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding global or thread-local UI state.
- Calling the store or the network from render.
- Showing an empty list when a load fails.

## Review Triggers

GPUI provides element-owned animation state or async data primitives.

## Code

`crates/dito-app/src/ui/scope.rs`, `crates/dito-app/src/settings/query_state.rs`, `crates/dito-app/src/settings/page_query.rs`, `crates/dito-app/src/services/`.
