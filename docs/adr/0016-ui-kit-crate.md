# ADR-0016: The shared UI kit becomes its own crate, in stages

- **Status**: accepted
- **Date**: 2026-11-20

## Context

`crates/dito-ui/src` holds the controls, motion, typography and color tokens that every window is built from: about 6,800 lines in 48 files, used by 134 of the app's other source files. `crates/dito-app/src/ui` is the app-facing facade and contains the app-bound controls. They follow the design file closely and took long to tune, and nothing in them is specific to dictation. The module still reaches into the app in four places (the palette, the icon set, the UI language for typefaces, and the Reduce Motion reading), and shared controls keep appearing inside single screens (the slider, the scroll edge fade, the loading skeleton, the Markdown renderer) because nothing marks where the kit ends.

## Decision

The generic part of the UI module becomes a workspace crate that depends only on `gpui-kit`, so other programs can build on it. The app's `ui` module re-exports the crate and keeps the components that need app or core types, so screen code keeps one import path.

The work is staged, and each stage leaves the app looking and behaving the same:

1. Cut the four ties inside the app. The UI module owns its palette (the app's palette contains it and adds the tokens single screens use), owns the icon set (the shared Lucide library of `design:components`), takes font family names from the app, and reads Reduce Motion through a source the app installs.
2. Move the generic files into the crate, with their tests and a gallery example that shows every control in the light and dark palettes.
3. Bring shared controls that live in screens into the crate.
4. Move the crate to its own repository once its API has gone some weeks without a breaking change, a second program uses it, and the shared pages of the design file can live with it.

What belongs in the kit: anything drawn from `design:foundations`, `design:components` and `design:motion` that needs no knowledge of dictation. What stays in the app: components that take `dito-core` types or produce localized copy (error summaries, connection-test status, history labels), tokens that serve one screen, and the HUD's own glyphs.

## Alternatives Considered

### A separate repository now

- Pros: An independent project from the first day
- Cons: The design file that leads every UI change stays in this repository, each control change would need two pull requests, and the function-style API would be frozen by its first outside user
- Why not chosen: The controls still change often; a workspace crate gives the boundary without that cost, and another program can already depend on it through a git dependency

### Keep the module inside the app

- Pros: No migration
- Cons: App-bound and generic components stay mixed, shared controls keep landing in screens, and nothing can be reused
- Why not chosen: The boundary is cheap to draw while there are only four ties

## Consequences

The compiler enforces the boundary once the crate exists; until then a change to the UI module is reviewed against the rule "no app or core path in a generic file". The design file remains the source of truth for the kit (ADR-0001). The kit and the app share one `gpui-kit` version through the workspace. The kit has two process-wide settings that the app supplies at startup, the selected font families and the Reduce Motion source; stage 1 records them in ADR-0013, which lists every allowed process-wide UI setting.

## Pitfalls

Moves that look reasonable but go against this decision:

- Importing `dito-core`, `dito-platform` or an app module into a generic UI file.
- Adding a token to the UI palette because one screen needs it.
- Making screen code longer to satisfy the boundary. When a helper needs app-only data, keep an app-bound wrapper with the old signature.
- Moving the crate to its own repository before the three conditions hold.

## Review Triggers

A second program wants the kit; the kit's API has been stable for some weeks; FigCraft can share a component library between files.

## Code

`crates/dito-ui/src/`, `crates/dito-app/src/ui/`, `crates/dito-app/src/theme.rs`, `crates/dito-app/src/assets.rs`.
