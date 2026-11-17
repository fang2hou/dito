# AGENTS.md

Guidance for AI agents working in this repository. Read this before making changes.

Dito is a macOS dictation app in Rust and GPUI: press a shortcut, speak, and polished text is typed at the cursor. It is a personal, daily-use tool in fast early development.

## Commands

```bash
mise install                                 # Rust, Python, uv and ruff at the pinned versions (mise.toml)
mise run dev:mock                            # run with mock cloud, mic and data; never types into other apps
mise run dev                                 # run with the real microphone, models and providers
mise run release                             # optimized release bundle, launched from target/bundle
mise run release:install                     # install the release bundle to ~/Applications, creating the signing identity if needed
mise run hooks:install                       # once per clone: commit hooks (prek) run the checks and clippy before every commit
mise run bench:lint                          # ruff and ty for the Python bench project; mise run bench:test runs its tests
cargo test -p <crate>                        # tests for the crate you changed
cargo test -p dito-app --no-default-features --test i18n_coverage  # after editing crates/dito-core/src/i18n/messages.toml
cargo fmt --all -- --check                   # format check
mise run lint:rust                           # clippy, also with the mock and screenshots features (the one lint command)
./scripts/check.sh fast                      # fmt and fast tests (`mise run check`); the commit hooks run it
./scripts/check.sh full                      # adds compile checks of every build flavor
mise run check:design-refs                   # every design:<id> reference resolves against design/sections.toml
figcraft-cli render design/dito.figcraft -o out.png --page "<page>"  # render a design page headlessly
```

First-time setup (Xcode Metal toolchain, signing certificate) is in `docs/IMPLEMENTATION.md`.

## Engineering Standards

This project follows the shared engineering guidelines: <https://github.com/fang2hou/ai-coding-guidelines/blob/main/PORTAL.md>. Read the portal's reading recipe for your task first; repository documentation wins over remembered summaries.

Project-specific overrides:

- Tool versions and common tasks live in `mise.toml`, commit hooks in `prek.toml`; Rust code runs through `cargo` and `scripts/check.sh`, the Python bench project through uv.
- `bench/` pins Python 3.15, newer than the guideline default of 3.12, at the owner's request; every dependency it uses ships 3.15 wheels or builds from source.
- Tests stay light (see Project Conventions); this narrows the testing guideline for a fast-moving personal app.
- Architecture, development (`IMPLEMENTATION.md`) and contributing docs live in `docs/` instead of the repository root, at the owner's request.

## Layout

- `crates/dito-app`: GPUI app (`hud/`, `settings/`, `onboarding/`, shared `ui/`).
- `crates/dito-core`: dictation pipeline, config, providers, i18n strings (`i18n/messages.toml`).
- `crates/dito-platform`: macOS integration (hotkeys, text insertion, permissions, windows).
- `crates/dito-audio`, `crates/dito-local`, `crates/dito-runtimes`: capture, local model management, on-device inference.
- `crates/dito-testkit`: mock cloud servers.
- `bench/`: Python benchmark harness for ASR and polish models (`dito-bench`), with stored results per chip. See `bench/README.md`.
- `design/`: `dito.figcraft` (source of truth for UI and motion), its conventions, `sections.toml` (the ids behind `design:<id>`), `features.md` (goals and features), and `asr-templates/` (compiled into the app; never move or delete).
- `docs/`: architecture, glossary, implementation, debugging, contributing and ADRs. `docs/workflow/`: step-by-step workflows. `scripts/`: bundling, install, signing, checks.

## Boundaries

Always:

- Reuse shared UI controls in `crates/dito-app/src/ui` before composing screen-specific chrome. Extend the shared component when a new content type needs the same behavior; keep sizing, arrow and hover animation, disabled states, and input focus ownership in that shared path.

- Treat `design/dito.figcraft` as the source of truth: the design leads and the code follows it. Change the design first for any UI change, through the `figcraft-live` MCP, and save it with `file.save` after every change without asking.
- Before editing the design, decide whether the change is a shared component or belongs to one screen. Shared components live in `design:components` and `design:motion`; screen-specific parts live in that screen's section under `design:settings`, `design:hud` or `design:onboarding`. Refer to sections by these ids (`design/sections.toml`), never by number; `mise run check:design-refs` verifies them.
- Change code only after the user has reviewed the design section in FigCraft and approved it, then make the code match it and name the section in your reply.
- After adding an icon or changing an icon's shape, run the `icon-keylines` skill (`.agents/skills/icon-keylines`) so its optical size and centering follow the shared rule.
- Run `./scripts/check.sh fast` and `mise run lint:rust` before handing work back; the commit hooks run both.
- Follow the main-only flow (`docs/CONTRIBUTING.md`): branch from `main` as `<type>/<kebab-name>` (for example `feat/menu-bar-icon`), open pull requests into `main` with a Conventional Commit title, and never push to `main` directly. CI rejects other branch names and titles.
- Keep screen copy identical to the `ja` values in `messages.toml`.

Never:

- Join text fragments with `·`, `|`, `｜` or a separating `・`, in FigCraft text, layer names, UI copy, docs or comments (see Project Conventions).
- Decide color, contrast or animation feel with a script.
- Bring generic web UI patterns (side accent stripes, filled badges) into the design file; build from the file's existing boards and palette.
- Mention internal company material (brand guides, company or client names, internal hosts) in the design file, docs, code, comments or sample data. The repository may become public.

Ask first:

- Visual or motion judgments you can't settle from the design and code. Before asking, have the app running and FigCraft open on the section, and say exactly what to do and where to look.
- New dependencies, and changes to signing or permissions scripts.

## Confirmed Language Policy

| Item                                    | Value                                               |
| --------------------------------------- | --------------------------------------------------- |
| Conversation                            | Follows the user (Simplified Chinese for the owner) |
| Code, comments, commits                 | English                                             |
| Repo docs and design-file documentation | English                                             |
| UI language                             | Japanese by default; English, 简体中文, 繁體中文, 한국어, Монгол, and Français selectable |
| Tone                                    | Plain, short, natural                               |

Do not infer UI language from conversation language.

## Project Conventions

- Comments: prefer clear identifiers and small functions over comments; comment only non-obvious invariants or constraints ("why", never "what"), name `Tr` variants or `design:<id>` sections instead of quoting localized UI strings, and keep configuration files comment-free. `i18n_coverage` enforces zero CJK in Rust comments.
- Writing: use commas for lists, a colon between a label and its value (`190 ms: about 108 pt`), parentheses for a short qualifier (`General (dark)`), or a full sentence. Wrong: `7 | In use · gold border + label`. Right: `7. In use, with a gold border and label`. Fixed Japanese compounds such as `大文字・小文字` are fine; Markdown table pipes are syntax.
- Tests: write one when logic is easy to break and hard to notice in the app (parsing, config migration, state machines, i18n coverage, protocols). No defensive tests for layout, every error branch or hypothetical misuse; delete a test that blocks a legitimate change and protects nothing a user would notice.
- Scripts may check exact geometry (alignment, sizes, bounds, overflow). Color and motion feel are judged by the user in the running app.
- Design file structure, naming, note cards and where shared versus screen-specific changes go: `design/README.md`.
- Before changing code an ADR covers, read it in `docs/adr/`; update or supersede it in the same change instead of working around it.

Depth: `docs/GLOSSARY.md` for terms and where each concept lives, `docs/DEBUGGING.md` to run and diagnose, `docs/workflow/` for the UI and code-fix workflows, `docs/README.md` for architecture, implementation, contributing and ADRs, `design/README.md` and `design/features.md` for design.
