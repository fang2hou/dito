# ADR-0010: Each setting has exactly one home

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Settings edited from several places drift: the menu bar, onboarding and settings each grow their own copies, and users can't tell which one wins.

## Decision

`ConfigStore` is the only source of truth: atomic writes, change notifications, and permissive loading so old files keep working. Each setting is edited in one place in the settings window. Onboarding only sets initial values; the menu bar never changes settings. The HUD language selector, which changes the speech language, is the one documented exception. The list of shortcuts available in the HUD is edited in General only. An empty list hides the switcher without changing the speech language.

## Alternatives Considered

### Quick toggles everywhere

- Pros: Convenient
- Cons: Duplicated controls and unclear ownership
- Why not chosen: Rejected

## Consequences

New quick actions must link to the setting's home rather than duplicate it. New fields need sensible defaults.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding settings controls to the menu bar or HUD (beyond the documented exception).
- Writing config outside `ConfigStore::update`.
- Adding required config fields without defaults.

## Review Triggers

A quick setting is requested often enough to justify a second home.

## Code

`crates/dito-core/src/config/store.rs` (`ConfigStore`), `crates/dito-app/src/menubar.rs`.
