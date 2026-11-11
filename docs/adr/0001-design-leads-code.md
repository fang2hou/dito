# ADR-0001: The design file leads the code

- **Status**: accepted
- **Date**: 2026-11-05

## Context

UI and motion drift when code is allowed to redefine them: whoever edits last decides the look, and nobody reviews motion before it ships. Values copied into prose go stale.

## Decision

`design/dito.figcraft` is the source of truth for UI and motion; code implements approved design sections.

- Every UI change starts in the design. Shared components and motion are edited in `design:components` and `design:motion`; anything that belongs to one screen is edited in that screen's section.
- Code changes start only after the owner has reviewed the section.
- Screen copy in the design equals the `ja` values in `messages.toml`.

## Alternatives Considered

### Code first, design as illustration

- Pros: Faster for small tweaks
- Cons: No place to review look and motion; the design rots
- Why not chosen: Rejected

## Consequences

Two steps per UI change, one review. In return the design file can be read as the spec, and changes are judged on something visible.

## Pitfalls

Moves that look reasonable but go against this decision:

- Updating the design to match code that drifted, instead of fixing the code or deciding the change in the design first.
- Putting a shared component's change in one screen, or a screen-specific change on the shared pages.

## Review Triggers

A second designer joins, or the design moves to another tool.

## Code

`design/README.md`, `docs/workflow/ui-adjustment.md`.
