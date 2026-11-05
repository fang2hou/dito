# ADR-0003: Never lose what the user said

- **Status**: accepted
- **Date**: 2026-11-05

## Context

A dictation is effort the user can't repeat exactly. Every failure mode (polish errors, no text field, time limits, cancelled recordings) is a chance to throw it away.

## Decision

Once speech is recognized, it reaches the user one way or another.

- A polish failure inserts the recognized text and records the error in History.
- With nowhere to type, the text goes to the clipboard and the HUD says Copied.
- The 5-minute limit finishes and processes the recording; it never discards it.
- Every result is kept in History with its audio; cancelled recordings that contain speech are kept as Cancelled; errors keep the clip so Retry can run it again.
- Only silent or very short clips are dropped.

## Alternatives Considered

### Fail the whole dictation on any error

- Pros: Simpler states
- Cons: Throws away good transcripts
- Why not chosen: Rejected

## Consequences

History and audio grow without a retention policy; storage management is a future product decision, not a reason to drop data silently.

## Pitfalls

Moves that look reasonable but go against this decision:

- Turning polish errors into pipeline failures.
- Discarding a recording at the time limit or on a delivery problem.
- Deleting audio automatically without a user-visible setting.

## Review Triggers

Storage use becomes a problem for users; a retention setting is designed.

## Code

`crates/dito-core/src/pipeline.rs` (`run`), `crates/dito-core/src/controller.rs` (`MAX_RECORDING_DURATION`, `done`), `crates/dito-core/src/store.rs`.
