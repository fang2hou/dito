# Contributing

How a change lands. Setup lives in [IMPLEMENTATION.md](IMPLEMENTATION.md); rules for AI agents live in [AGENTS.md](../AGENTS.md).

## Ground Rules

- The smallest complete change that solves the request; no unrelated cleanup.
- UI changes start in the design and need the owner's approval before code ([adjust the UI](workflow/ui-adjustment.md)).
- No new dependency without a reason the change can't do without.
- Nothing internal: no company or client names, internal hosts or internal documents in code, docs, the design file or sample data.

## Change Workflow

1. Branch from `main`.
2. Implement; keep `./scripts/check.sh fast` and clippy green.
3. Commit with Conventional Commits in English, for example `feat(hud): pop the language selector like a tooltip` or `fix(onboarding): put the caret before the placeholder`.
4. Open a pull request. `main` is protected; a person reviews and merges.

## Pull Request Descriptions

Every description states:

- **Purpose**: what the change is for.
- **Impact**: what users or code paths are affected.
- **Context**: background, linked design sections or ADRs.
- **Risks**: what could go wrong.
- **Verification**: the checks run, and what was tried in the running app.

## Review Expectations

Reviewers check the requested behavior, scope, compatibility with [ARCHITECTURE.md](ARCHITECTURE.md) and the [ADRs](adr/README.md), that UI matches the approved design section, and that nothing internal or secret is included.
