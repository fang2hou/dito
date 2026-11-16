# ADR-0011: Japanese first, display languages compiled in

- **Status**: accepted
- **Date**: 2026-11-16

## Context

The app is built for Japanese users first; users in other supported UI locales (English, Simplified Chinese, Traditional Chinese, Korean, Mongolian, and French) must still get a complete UI. Missing or inconsistent strings show up only at runtime when lookups are dynamic.

## Decision

Japanese is the primary UI language and the design's language; English, Simplified Chinese (`zh`), Traditional Chinese (`zh-Hant`), Korean (`ko`), Mongolian (`mn`, Cyrillic), and French (`fr`) are complete UI translations. Strings are compiled from `messages.toml` into a typed enum; the build fails on missing languages or placeholder drift, and a test forbids string lookups and CJK literals in code. The display language and the speech/prompt language (`ja`, `en`, `zh`) are separate settings. Speech language names are translated into the UI language. The display-language selector retains self-names so it remains readable to someone switching from an unfamiliar locale. Fonts per language are decided in one place (bundled faces for Japanese, Latin, and Simplified Chinese; macOS built-in faces PingFang TC, Apple SD Gothic Neo, and Helvetica Neue for Traditional Chinese, Korean, and Mongolian Cyrillic); no language adds letter spacing.

## Alternatives Considered

### Runtime string tables

- Pros: Hot-swappable
- Cons: Missing strings found by users
- Why not chosen: Rejected

## Consequences

The native renderer uses the bundled fonts at their declared weights without color-dependent macOS smoothing dilation. A process-local volatile preference is set before GPUI initializes; no persistent user or system preference is changed.

Adding a language means adding it to the TOML and choosing fonts; screens don't branch on language. The intended default display language is Japanese (today the code follows the system).

## Pitfalls

Moves that look reasonable but go against this decision:

- Hard-coding UI strings or CJK literals in Rust.
- Designing in English and translating to Japanese afterwards.
- Branching on language inside screens instead of in the typography helpers.

## Review Triggers

A right-to-left script or runtime-downloaded locale pack is requested.

## Code

`crates/dito-core/build.rs`, `crates/dito-core/src/i18n/messages.toml`, `crates/dito-app/tests/i18n_coverage.rs`, `crates/dito-app/src/ui/typography.rs`.
