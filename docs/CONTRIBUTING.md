# Contributing

How a change lands. Setup lives in [IMPLEMENTATION.md](IMPLEMENTATION.md); rules for AI agents live in [AGENTS.md](../AGENTS.md).

## Ground Rules

- The smallest complete change that solves the request; no unrelated cleanup.
- UI changes start in the design and need the owner's approval before code ([adjust the UI](workflow/ui-adjustment.md)).
- No new dependency without a reason the change can't do without.
- Nothing internal: no company or client names, internal hosts or internal documents in code, docs, the design file or sample data.

## Git Flow

| Branch | Purpose | Created from | Merges into |
| --- | --- | --- | --- |
| `main` | Released code; every commit is a release or a hotfix | none | none |
| `develop` | Integration of finished work; the default base for pull requests | `main` | none |
| `<type>/<kebab-name>` | One feature or change, for example `feature/menu-bar-icon` or `fix/caret-position` | `develop` | `develop` |
| `release/<version>` | Stabilization of one release; only fixes, docs, tests and build changes | `develop` | `main`, then back into `develop` |
| `hotfix/<name>` | Urgent fix of a released version | `main` | `main`, then back into `develop` |

`<type>` is one of `feature`, `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`. A pull request into a topic branch is allowed, so work can be stacked.

Merge strategy:

- Topic branch into `develop` or a topic branch: squash merge. The pull request title becomes the commit message, so it must be a Conventional Commit.
- `release/*` or `hotfix/*` into `main`: merge commit, never squash, so `main` stays an ancestor of `develop` after the back-merge.
- After every merge into `main`, open a pull request from `main` into `develop` (a normal merge, not squash) so `develop` gets the release or hotfix commits. Do this before starting the next release.

Releasing:

1. Branch `release/<version>` from `develop`, set the workspace version in `Cargo.toml`, run `mise run check:full` and `mise run bundle` locally, and open a pull request into `main`.
2. Merge it, then tag the merge commit `v<version>` on `main` and push the tag.
3. The Release workflow verifies the tag, builds the app bundle and creates a draft GitHub release with the zipped app. The bundle is ad-hoc signed, so macOS will ask for Microphone and Accessibility again; use `mise run release:install` for a build signed with your own `dito-dev` identity.
4. Review the draft and publish it, then open the back-merge pull request.

## Change Workflow

1. Branch from `develop` as `<type>/<kebab-name>`.
2. Implement; the commit hooks run the checks, and `mise run lint` covers everything at once.
3. Commit with Conventional Commits in English, for example `feat(hud): pop the language selector like a tooltip` or `fix(onboarding): put the caret before the placeholder`.
4. Open a pull request into `develop` with a Conventional Commit title. `main` and `develop` are protected; a person reviews and merges.

## Checks

Checks run on your machine at commit time through [prek](https://prek.j178.dev) (`prek.toml`); GitHub Actions does not run tests. Install the hooks once per clone with `mise run hooks:install`.

| Hook | Runs when | Command |
| --- | --- | --- |
| Secret scan, merge-conflict, TOML, JSON, YAML and large-file checks (500 KB, or 8 MB for benchmark records under `bench/results/`) | every commit | gitleaks and prek built-ins |
| Rust format and tests | Rust, Cargo or mise files change | `mise run check` (`scripts/check.sh fast`) |
| Rust clippy | Rust, Cargo or mise files change | `mise run lint:rust` |
| Bench lint, types and tests | `bench/` or mise files change | `mise run bench:check` |
| Workflow lint | `.github/workflows/` changes | `mise run lint:actions` |
| Commit message is a Conventional Commit | every commit | `mise run hooks:commit-msg` |

`mise run lint:rust` is the one clippy command: it lints the workspace, then again with the `mock` and `screenshots` features so the code behind them is covered. Docs and scripts refer to it instead of spelling out the cargo command. `mise run hooks` runs every hook against all files. Hooks only see what is staged, and `git commit --no-verify` skips them, so nothing server-side backs them up: do not skip them.

GitHub Actions only enforces what cannot be checked locally. Workflows live in `.github/workflows` and call `mise run` tasks.

| Workflow | Runs on | Does |
| --- | --- | --- |
| Pull request rules | every pull request, including title edits | Conventional Commit title, branch flow |
| Release | tags `v*.*.*` | tag matches the workspace version and is on `main`, then a draft release with the zipped app |

## Pull Request Descriptions

Every description states:

- **Purpose**: what the change is for.
- **Impact**: what users or code paths are affected.
- **Context**: background, linked design sections or ADRs.
- **Risks**: what could go wrong.
- **Verification**: the checks run, and what was tried in the running app.

## Review Expectations

Reviewers check the requested behavior, scope, compatibility with [ARCHITECTURE.md](ARCHITECTURE.md) and the [ADRs](adr/README.md), that UI matches the approved design section, and that nothing internal or secret is included.
