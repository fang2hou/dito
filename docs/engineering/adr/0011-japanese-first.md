# ADR-0011: Japanese first, three languages compiled in

- **Status**: accepted
- **Date**: 2026-11-05

## Context

The app is built for Japanese users first; English and Simplified Chinese users must still get a complete UI. Missing or inconsistent strings show up only at runtime when lookups are dynamic.

## Decision

Japanese is the primary UI language and the design's language; English and Simplified Chinese are complete translations. Strings are compiled from `messages.toml` into a typed enum; the build fails on missing languages or placeholder drift, and a test forbids string lookups and CJK literals in code. The display language and the speech language are separate settings. Fonts and letter spacing per language are decided in one place.

## Alternatives Considered

### Runtime string tables

- Pros: Hot-swappable
- Cons: Missing strings found by users
- Why not chosen: Rejected

## Consequences

Adding a language means adding it to the TOML and choosing fonts; screens don't branch on language. The intended default display language is Japanese (today the code follows the system).

## Pitfalls

Moves that look reasonable but go against this decision:

- Hard-coding UI strings or CJK literals in Rust.
- Designing in English and translating to Japanese afterwards.
- Branching on language inside screens instead of in the typography helpers.

## Review Triggers

A fourth language is requested.

## Code

`crates/dito-core/build.rs`, `crates/dito-core/src/i18n/messages.toml`, `crates/dito-app/tests/i18n_coverage.rs`, `crates/dito-app/src/ui/typography.rs`.
