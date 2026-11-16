# ADR-0005: Reliable insertion beats simple insertion

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Typing into other apps is the product's core promise and its least controllable part: focus moves after clicks, Chromium and Electron drop fast synthetic keys, editors rewrite text, GPU-rendered editors expose no accessibility text, and clipboard managers race the paste. A single type or paste call failed in all of these.

## Decision

Delivery is a verified, bounded flow: try the configured method, check the cursor, continue only what is missing, switch method once if nothing landed, and fall back to the clipboard last. Never retry text that already landed; treat unverifiable targets as success; restore the clipboard from a full snapshot only if nothing else wrote to it. The policy lives in `dito-core::insert`, the macOS mechanics in `dito-platform::insert`. Resolve focus through both the system and the focused application. Web editors may expose the editable ancestor of the focused node. Require focus evidence before selecting a descendant, and verify or retry against the same accessibility element.

Dito's own windows use the app-side sink because their editors are not foreign accessibility targets. In `design:onboarding`, the focused Try It editor accepts the result through its ordinary selection replacement API, preserving undo history and existing text outside the selection. The sink claims the request before changing the editor and reports insertion only after that synchronous edit. Only this delivery completes the onboarding trial; clipboard fallback, unrelated history and manual typing do not. Tools and Settings retain their focused-field paste route.

## Alternatives Considered

### One type or paste call

- Pros: Simple
- Cons: Every failure listed above
- Why not chosen: Rejected

### Retry the whole text

- Pros: Simple
- Cons: Duplicates what landed
- Why not chosen: Rejected

## Consequences

Delivery is the most complex part of the app and should stay that way; new target quirks are handled inside the flow, not by bypassing it.

## Pitfalls

Moves that look reasonable but go against this decision:

- Simplifying delivery to one call, or retrying the full text after a partial insert.
- Treating an unverifiable result as a failure.
- Reading a caret that did not move as proof that nothing landed. Terminals such as Ghostty report the selection at 0 whatever is typed, so a field that changed under an unmoved caret is unverifiable, not failed. Searching the field for the text is no substitute: scrollback matches old text, and short text gives too little evidence.
- Comparing a field's content sooner than 500 ms after reading it. Ghostty serves its accessibility text from a cache for that long.
- Restoring the clipboard on a timer, as text only, or over someone else's write.
- Sending keys from the shared event source (a held right ⌘ turns them into shortcuts) or sending chunks faster than every 10 ms.
- Dropping the path for apps without accessibility text (Zed and other GPU-rendered editors).

## Review Triggers

A new class of target app fails, or macOS offers a supported text insertion API.

## Code

`crates/dito-core/src/insert.rs` (`Flow`), `crates/dito-platform/src/insert/deliver.rs` (`deliver_text`).
