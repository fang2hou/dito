# ADR-0012: Fix model behavior with structure, not longer prompts

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Speech models echo their hotword list on silence, and polish models mix reference content into the output or answer dictated questions. Adding instructions to the prompts didn't stop either.

## Decision

Model behavior is controlled through structure: follow each model's own prompt format, fence reference content (vocabulary, style samples) in escaped tags, gate silent clips before any engine, and detect known failure shapes (such as a vocabulary echo) and re-run without the trigger.

## Alternatives Considered

### Longer instructions

- Pros: Easy
- Cons: Didn't work; grows with every model
- Why not chosen: Rejected

## Consequences

Each new model needs its own format and failure checks; heuristics have limits and are documented next to the code.

## Pitfalls

Moves that look reasonable but go against this decision:

- Fixing hallucinations by adding instructions or by deleting hotwords from transcripts.
- Removing the silent-clip gate.
- Putting chat instructions into a speech model's audio turn.

## Review Triggers

Models with reliable built-in voice detection and instruction following replace the current ones.

## Code

`crates/dito-runtimes/src/llama.rs` (`asr_prompt`, `is_vocabulary_echo`), `crates/dito-core/src/prompts.rs`, `crates/dito-core/src/audio.rs`.
