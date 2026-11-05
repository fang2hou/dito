# ADR-0006: Only Microphone and Accessibility

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Every system permission is a trust cost and a setup step. Dictation needs the microphone, and typing into other apps and hearing a global shortcut need Accessibility.

## Decision

Dito asks for Microphone and Accessibility and nothing else. The global shortcut uses an active event tap, which needs only Accessibility and can also swallow Esc and shortcut keys; it recognizes lone modifiers such as right ⌘ and treats a quick following key as typing.

## Alternatives Considered

### Listen-only event tap

- Pros: Passive
- Cons: Needs Input Monitoring and can't swallow Esc or combos
- Why not chosen: Rejected

### GPUI key handling

- Pros: No permission
- Cons: Can't see lone modifiers or keys while other apps are focused
- Why not chosen: Rejected

## Consequences

Features that would need another permission (screen recording, Input Monitoring, contacts) need an explicit product decision and an update to this ADR.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding an Input Monitoring step or switching to a listen-only tap.
- Adding a feature that silently depends on a new permission.

## Review Triggers

A feature is proposed that can't work within these two permissions.

## Code

`crates/dito-platform/src/permissions.rs`, `crates/dito-platform/src/tap.rs`, `crates/dito-platform/src/hotkey.rs`.
