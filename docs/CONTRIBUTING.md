# Contributing

How a change lands. Setup lives in [IMPLEMENTATION.md](IMPLEMENTATION.md); rules for AI agents live in [AGENTS.md](../AGENTS.md).

## Ground Rules

- The smallest complete change that solves the request; no unrelated cleanup.
- UI changes start in the design and need the owner's approval before code ([adjust the UI](workflow/ui-adjustment.md)).
- No new dependency without a reason the change can't do without.
- Nothing internal: no company or client names, internal hosts or internal documents in code, docs, the design file or sample data.

## Branch Flow

`main` is the only long-lived branch. It is protected: every change reaches it through a pull request that a person reviews and merges.

| Branch | Purpose | Created from | Merges into |
| --- | --- | --- | --- |
| `main` | Integration of finished work and the source of every release | none | none |
| `<type>/<kebab-name>` | One feature or change, for example `feature/menu-bar-icon` or `fix/caret-position` | `main` | `main` |

`<type>` is one of `feature`, `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`. A pull request into a topic branch is allowed, so work can be stacked.

Every pull request is squash merged. The pull request title becomes the commit message, so it must be a Conventional Commit.

Releasing:

1. Branch `chore/release-<version>` from `main`, set the workspace version in `Cargo.toml`, run `mise run check:full` and `mise run bundle` locally, and open a pull request into `main`.
2. Merge it, then tag the merge commit `v<version>` on `main` and push the tag.
3. The Release workflow verifies the tag, builds the app bundle and creates a draft GitHub release with the zipped app. The bundle is ad-hoc signed, so macOS will ask for Microphone and Accessibility again; use `mise run release:install` for a build signed with your own `dito-dev` identity.
4. Review the draft and publish it.

## Change Workflow

1. Branch from `main` as `<type>/<kebab-name>`.
2. Implement; the commit hooks run the checks, and `mise run lint` covers everything at once.
3. Commit with Conventional Commits in English, for example `feat(hud): pop the language selector like a tooltip` or `fix(onboarding): put the caret before the placeholder`.
4. Open a pull request into `main` with a Conventional Commit title, using the repository pull request template. `main` is protected; a person reviews and merges.

## Checks

Checks run on your machine at commit time through [prek](https://prek.j178.dev) (`prek.toml`). GitHub Actions independently runs the fast tests, CI clippy set and text checks on every pull request into `main` and every push to `main`. Install the hooks once per clone with `mise run hooks:install`.

| Hook | Runs when | Command |
| --- | --- | --- |
| Secret scan, merge-conflict, TOML, JSON, YAML and large-file checks (500 KB, 8 MB for benchmark records under `bench/results/`, or 24 MB for intro films under `assets/intro/`) | every commit | gitleaks and prek built-ins |
| Rust format and tests | Rust, Cargo, mise, scripts/check.sh, compiled design templates or app icon files change | `mise run check` (`scripts/check.sh fast`) |
| Rust clippy | Rust, Cargo, mise, compiled design templates or app icon files change | `mise run lint:rust` |
| Design reference check | `design/sections.toml`, `design/dito.figcraft`, or `.rs`, `.md`, `.py`, `.sh`, `.toml` files change (the suffixes the check scans) | `mise run check:design-refs` |
| Shell lint | `scripts/*.sh` files change | `mise run lint:shell` |
| Bench lint, types and tests | `bench/` or mise files change | `mise run bench:check` |
| Workflow lint | `.github/workflows/` changes | `mise run lint:actions` |
| Commit message is a Conventional Commit | every commit | `mise run hooks:commit-msg` |

`mise run lint:rust` is the complete local clippy command: it lints the workspace, then again with the `mock` and `screenshots` features so the code behind them is covered. `mise run lint:rust:ci` uses the same mock and screenshot coverage over the crate set that does not require local runtimes. `mise run hooks` runs every hook against all files. Hooks only see what is staged, and `git commit --no-verify` skips them, so do not skip them even though GitHub Actions provides a server-side check.

Workflows live in `.github/workflows` and call `mise run` tasks.

| Workflow | Runs on | Does |
| --- | --- | --- |
| Pull request rules | every pull request, including title edits | Conventional Commit title, branch flow |
| Checks | pull requests into `main` and pushes to `main` | actionlint, shellcheck, design reference checks, fast Rust tests, CI clippy |
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
