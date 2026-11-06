# ADR-0013: UI state belongs to its view and never blocks rendering

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Global animation maps replayed old animations and leaked; synchronous queries in render froze the settings window. Both came from state with no clear owner.

## Decision

Each window, page and onboarding step owns its motion state (`MotionScope`, reached through weak handles and reset when the view leaves). Data for pages loads in the background into snapshots with request tickets; stale results are dropped and failures keep the last data. Nothing slow runs in render.

## Alternatives Considered

### Global registries and inline queries

- Pros: Less code
- Cons: Replays, leaks and frozen UI
- Why not chosen: Replaced

## Consequences

New views follow the same pattern: own your state, load in the background, apply only current results.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding global or thread-local UI state.
- Calling the store or the network from render.
- Showing an empty list when a load fails.

## Review Triggers

GPUI provides element-owned animation state or async data primitives.

## Code

`crates/dito-app/src/ui/scope.rs`, `crates/dito-app/src/settings/query_state.rs`, `crates/dito-app/src/settings/page_query.rs`, `crates/dito-app/src/services/`.
