# ADR-0004: The HUD never takes focus and never gets in the way

- **Status**: accepted
- **Date**: 2026-11-05

## Context

The HUD floats over the app the user is typing into. If it takes focus, text goes to Dito instead of the target. If it covers menus or blocks clicks, it breaks the user's work. Earlier versions used a normal window: clicking ✓ or Stop made the HUD key, and dictations silently landed on the clipboard.

## Decision

The HUD is a non-activating panel that can't become key on click, sits above app windows but below menus and tooltips, lets clicks pass through everywhere except its controls, and closes when the dictation ends. Hover and click-through come from polling the pointer, because the panel receives no mouse events while it lets clicks through. Its surfaces are opaque with one soft shadow.

## Alternatives Considered

### Normal window

- Pros: Simple
- Cons: Clicks make it key and text lands on the clipboard
- Why not chosen: Rejected after failures

### Floating window

- Pros: Stays above
- Cons: No hover while unfocused, ties with other apps' panels
- Why not chosen: Rejected

### System blur backdrop

- Pros: Native look
- Cons: Tint and radius can't be controlled
- Why not chosen: Rejected

## Consequences

Every new HUD control must work through the polled hover and click-through. Apply AppKit window moves after releasing the GPUI app borrow because the move callback runs synchronously. The HUD advances on its display link and opts out of inactive-window throttling. Quiet frames can skip drawing. New HUD features must not require focus (no text fields in the HUD).

## Pitfalls

Moves that look reasonable but go against this decision:

- Switching to a Normal or Floating window, or removing `configure_nonactivating_panel`.
- Replacing the pointer poll with hover events.
- Raising the HUD above menus (PopUp's default level).
- Adding controls to the HUD that need keyboard focus.

## Review Triggers

GPUI gains a native non-activating panel or window levels; macOS changes panel focus rules.

## Code

`crates/dito-app/src/hud/window.rs`, `crates/dito-platform/src/system/hud_window.rs` (`HudNativeWindow::keep_panel_inactive`, `pointer`, `set_click_through`).
