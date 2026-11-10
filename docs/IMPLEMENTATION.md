# Implementation

How to build, run, test and change Dito, for people and AI agents. Product behavior lives in [design/features.md](../design/features.md); structure lives in [ARCHITECTURE.md](ARCHITECTURE.md).

## Setup

| Tool | Purpose | Managed via |
| --- | --- | --- |
| Rust 1.99.1 with rustfmt and clippy | Compiler and checks | mise (`mise.toml`) |
| Python 3.15, uv, ruff and ty | The `bench/` benchmark project | mise (`mise.toml`); packages through uv |
| Xcode and its Metal Toolchain | GPUI shaders, `actool` and `ictool` for the icon | Xcode (`xcodebuild -downloadComponent MetalToolchain`) |
| cmake | llama.cpp and sherpa-onnx builds | Homebrew |
| `dito-dev` signing identity | Stable permissions across rebuilds (see Engineering Traps) | `mise run certs` (also run by `release:install`), once per machine |

If only the Command Line Tools are selected, export `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`.

## Commands

```bash
mise install                                  # tools at the pinned versions
mise run dev:mock                             # whole app with mocks (builds with the mock feature); never types into other apps
mise run dev                                  # real mode (real microphone, models and providers)
mise run release                              # optimized release bundle at target/bundle/Dito.app, launched
mise run release:install                      # release bundle installed to ~/Applications/Dito.app
mise run check                                # ./scripts/check.sh fast: fmt, unit tests, mock-server tests, i18n coverage
mise run lint:rust                            # clippy for the workspace, again with the mock and screenshots features
mise run check:full                           # fast, plus compile checks of every build flavor
mise run lint                                 # lint:rust, ruff and ty for bench, actionlint for workflows
mise run hooks:install                        # once per clone: install the prek commit hooks (see CONTRIBUTING.md, Checks)
mise run hooks                                # run every hook against all files
mise run setup:macos                          # install the Xcode Metal Toolchain if it is missing
mise run format                               # cargo fmt and ruff format
cargo test -p <crate>                         # tests for one crate
cargo test -p dito-app --no-default-features --test i18n_coverage  # after editing messages.toml
```

Extra arguments reach the app: `mise run dev:mock -- --data-dir <empty dir>`.

Cargo features of `dito-app`: `runtimes` (on-device inference, the only default; needs cmake and the Metal toolchain), `mock` (the in-process mock cloud behind `--mock`, so release bundles do not link the test server) and `screenshots` (`--screenshots`). A flag whose feature is not compiled in fails with a usage error.

`mise run release` builds with `scripts/bundle.sh` (release profile with thin LTO and one codegen unit). `mise run release:install` runs `scripts/certs.sh` first (idempotent; the first run shows one macOS authorization prompt), refuses to install an ad-hoc signed build, quits any running Dito, replaces `~/Applications/Dito.app`, verifies the signature and opens the app.

The Python benchmark harness has its own tasks: `mise run bench:sync`, `bench:test`, `bench:lint`, `bench:format`, and `mise run bench <args>` for `dito-bench` (see `bench/README.md`).

## Workflow

1. Pick the matching workflow in [workflow](workflow/README.md): [adjust the UI](workflow/ui-adjustment.md) or [fix code](workflow/code-fix.md).
2. Read the ADRs that touch the change.
3. Make the smallest complete change.
4. Run `./scripts/check.sh fast` and `mise run lint:rust`; the commit hooks run both.
5. Commit following [CONTRIBUTING.md](CONTRIBUTING.md).

## Layout

- `crates/`: the workspace crates (see [ARCHITECTURE.md](ARCHITECTURE.md)).
- `design/`: `dito.figcraft` (UI and motion), `features.md`, the ASR connection config contract, and `asr-templates/` (compiled into the app).
- `assets/icon/`: the Icon Composer document.
- `bench/`: Python benchmark harness for ASR and polish models, with stored results.
- `mise.toml`: tool versions and the project tasks. `prek.toml`: commit hooks. `.github/`: PR rules, and the release workflow (see [CONTRIBUTING.md](CONTRIBUTING.md)).
- `scripts/`: `bundle.sh` (release app), `install.sh` (install to `~/Applications`), `quit.sh` (quit the running release Dito), `certs.sh` (signing identity), `check.sh` (checks), `dev-runner.sh` (wraps runs in `DitoDev.app`), `icon.sh` (icon build and preview), `setup-macos.sh` (Metal toolchain), `check-design-icons.py` (geometry check of the saved design), `check-flow.sh` and `check-release-tag.sh` (branch and tag rules for CI). `.agents/skills/` holds the project skills, including `icon-keylines`.
- `docs/`: this document, architecture, contributing and ADRs. `docs/workflow/`: step-by-step workflows.

## Coding Standards

- Code, comments and identifiers are English. UI copy comes only from `crates/dito-core/src/i18n/messages.toml`.
- Never join text fragments with `·`, `|`, `｜` or a separating `・`; write natural phrases.
- Keep platform code in `dito-platform` and logic in `dito-core`; the app wires them together.
- Every `unsafe` block states its safety conditions; AppKit calls stay on the main thread.
- Prefer a clearer ownership or type over a defensive check (see [fix code](workflow/code-fix.md)).
- gpui-kit component types (`gpui_kit::component::...`) are imported through `crate::ui`; only `crates/dito-app/src/ui` names those paths.

## Testing

- Tests live next to the code (unit) and in each crate's `tests/` (integration).
- Write a test when logic is easy to break and hard to notice in the app: parsing, config migration, state machines, insertion flow, i18n, protocols.
- Don't write defensive tests for layout, every error branch or hypothetical misuse. Look and motion are checked by a person in the running app.
- `check.sh` also runs `dito-app --test demo_run`: it needs a logged-in session but starts the app headless, so nothing appears on screen and focus stays where it is.
- Not run by `check.sh`, because they need the real Keychain, Accessibility or model files: `dito-platform --test keychain` and `--test loopback` (both `#[ignore]`, run with `-- --ignored`), and the ignored runtime regression tests (`DITO_ASR_MODEL_DIR`, `DITO_POLISH_MODEL_DIR`). `dito-runtimes` has no tests `check.sh` can run: its unit tests sit in the `llama` and `sherpa` modules, which need their features and the native builds.
- `check.sh` runs `dito-testkit --test bedrock` on a single thread, because those tests set AWS environment variables.

## Debugging

| Flag | Effect |
| --- | --- |
| `--mock` | Mock cloud server, memory secrets, synthetic microphone, data in `$TMPDIR/dito-mock` (build with `--features mock`) |
| `--log-sink` | Log results instead of typing them |
| `--real-mic` | Use the real microphone in mock mode |
| `--wav <file>` | Use a WAV file as the recording |
| `--data-dir <dir>` | Config and history location; an empty directory shows onboarding |
| `--lang <code>` | Pretend the system language is `ja`, `en` or `zh` |
| `--open-settings` | Open the settings window at launch. A launch opens no window by itself (see [ADR 0014](adr/0014-silent-launch.md)), so `mise run dev` and `dev:mock` pass this flag |
| `--demo-run` | Run one dictation headlessly and print it; no Dock icon, menu bar or window, never frontmost |
| `--screenshots <dir>` | Render screenshots (build with `--features screenshots`; narrow with `DITO_SHOTS_ONLY`); windows are never shown and never take focus |

- Logs go to stdout through `tracing`; the dev runner tails the app's output into the terminal. Override with `RUST_LOG`.
- Data lives in `~/Library/Application Support/com.fang2hou.Dito`: `config.toml`, `dito.db`, `audio/` and `models/`. Keys are in the Keychain under `com.fang2hou.dito`, one item per cloud polish configuration (`polish.<id>`) and per cloud speech profile (`asr.<id>`). The login item is `~/Library/LaunchAgents/com.fang2hou.dito.plist`.
- Debug builds (`mise run dev`, `cargo run`, tests) use a separate identity, `com.fang2hou.dito-dev` (data in `com.fang2hou.Dito-dev`, own Keychain items, login item, and Microphone and Accessibility grants), so they never read or change the installed app's config, history, keys or login item. API keys, models and permissions start empty there; point `models_dir` at the release folder to reuse downloaded models.
- `--data-dir` doesn't move the models folder; set `models_dir` for that.
- Use `--mock --log-sink` for any automated or agent run.
- Examples for manual checks: `cargo run -p dito-audio --example record`, `cargo run -p dito-platform --example probe`, `cargo run -p dito-platform --example press_shortcut -- 2` (presses right Command, to drive a running Dito), `cargo run -p dito-platform --example request -- accessibility` or `-- mic` (asks for a permission and watches its status), `cargo run -p dito-runtimes --example local_e2e`.

## Engineering Traps

Things that look removable or simplifiable but exist for a reason. Product-level judgments are in the [ADRs](adr/README.md).

- **Signing and the dev runner.** macOS ties Microphone and Accessibility grants to the code signature. `scripts/certs.sh` creates the `dito-dev` identity once; `.cargo/config.toml` routes runs through `scripts/dev-runner.sh`, which wraps the binary in `target/bundle/DitoDev.app`, signs it and launches it with `open`. A bare `target/debug/dito` gets no permission prompts and no accessibility answers, so never use it for permission or insertion work. `scripts/check.sh` bypasses the runner for automated tests.
- **Identifiers.** `com.fang2hou.dito` is the bundle id, signing identifier, Keychain service and login item label, and the data directory is `com.fang2hou.Dito`. Changing any of them resets permissions and orphans data and keys; change all of them together, with a migration plan, or not at all. They live in `dito-core/src/identity.rs` (and `scripts/bundle.sh`); debug builds take the `-dev` variants from there, and `scripts/dev-runner.sh` signs `DitoDev.app` with the same `com.fang2hou.dito-dev`.
- **Background frame rate.** The HUD is never key and Dito runs in the background, so macOS throttles it. While the HUD is open a `NapGuard` holds a user-interactive activity, and `HudView` drives its own 16 ms timer. Don't replace either with frame callbacks without measuring an unfocused HUD.
- **Unload models on quit.** `LocalEngines::unload_all()` runs on quit because llama.cpp's Metal backend crashes at exit when a model is still loaded.
- **Cancel reaches local inference through a stop flag.** The pipeline cancels by dropping the future; the local engine wrappers then set the `stop` flag their runtime call was given. A runtime that ignores the flag keeps computing in `spawn_blocking` and holds the model's lock, so every new runtime loop must check it.
- **Outcome timing lives in two crates.** The controller's `SUCCESS_FLASH` and `COPIED_FLASH` must keep the window alive through the HUD's processing catch-up, feedback and exit in `hud/timing.rs`; synchronize them when changing either clock. Include the longest catch-up when calculating the window lifetime.
- **Compiled-in design files.** `design/asr-templates/*.json` and `crates/dito-core/src/provider/asr-help/*.md` are compiled into `dito-core`. Moving or deleting them breaks the build. `design/asr-config.schema.json` is documentation only.
- **App icon.** `assets/icon/Dito.icon` is an Icon Composer document. `scripts/icon.sh compile` builds `Assets.car` and a flat `Dito.icns`; `preview` renders the appearances. Never call `setApplicationIconImage` in a bundled run (it flattens the Liquid Glass icon); only bare binaries set a PNG. Declare `features: ["refractivity", "specular-location"]` whenever those keys are used, and use `blur-material`, not `blur`, or Icon Composer won't open the file.
- **Dock icon policy.** With "Show icon in Dock" off, `main` starts GPUI with the Headless windowing request, which on macOS is only the Accessory activation policy (windows still draw). It must be set before the run loop; setting it later flashes the icon in the Dock at launch. `dock::sync` switches to Regular while the settings or onboarding window is open and back when the last one closes. The switch back runs one turn after the close callback: inside it, AppKit reports an active-status change for the removed window and GPUI logs `window not found`. Open a window, call `dock::sync`, then `cx.activate`; a Regular app that is not activated has no menu bar.
- **Mock mode types for real.** `--mock` without `--log-sink` types into the frontmost app. Agents and automated runs always add `--log-sink`.

## Known Limitations

- Cancelling local inference takes effect at the next llama.cpp token or the next sherpa-onnx chunk. A prompt evaluation, an audio encode or one sherpa-onnx decode that has already started runs to its end first.
- Bedrock polishing is covered only by the mock server, not by a real account.
- GitHub Actions enforces only the pull request rules and the release tag; tests and clippy run in the local commit hooks, which `--no-verify` skips.
- The display language defaults to the system language; the design calls for Japanese by default.

## Validation

The commit hooks run `./scripts/check.sh fast` and `mise run lint:rust`; run `./scripts/check.sh full` before a release build. A green run proves the commands passed, not that the behavior is right; check UI changes in the running app.

### Changing the database schema

The SQLite store behind `dito.db` versions its schema with `PRAGMA user_version`, which equals the number of entries in `MIGRATIONS` (`crates/dito-core/src/store.rs`) that have been applied. To change the schema, append a migration to that list; never edit or reorder one that has shipped, because existing databases have already run it. The store applies pending migrations when it opens the database and owns `user_version`, so nothing else writes it, and it refuses a database whose version is newer than the list. Add a test that opens a database at the previous version and checks that its rows survive and `user_version` reaches the new value.

### Adding a local model

The catalog in `dito-local` owns model files, pinned URLs, sizes, checksums, memory estimates, language-focus hints and the inference layout. For another checkpoint of an existing `ModelFormat`, add one catalog entry; downloads, verification, selection and engine lifecycle reuse the existing path. `ModelFormat` declares exact filenames for each role, so a projector or unrelated ONNX file cannot be mistaken for the main model. Keep external data files such as Haas-ja's `encoder.data` in the same download entry and directory.

A chat model of another family (Gemma, Llama, Phi) is a catalog entry with `ModelFormat::Chat` too, and its prompt format comes from the chat template embedded in the GGUF, so a file without one fails to load. Pass `disable_thinking: false` for it (the flag is the Qwen hybrid-thinking prefill), give it the `sampling` the family recommends (the values the model uses for any parameter the user leaves empty on the Models page; `Sampling::POLISH` is what the Qwen entries use), and check its output handling (`strip_think`, sampling) against real weights before it ships.

For a new architecture, add a `ModelFormat` variant and its adapter in `dito-runtimes`, declare its hotword capability, and extend the loader. Update ADR-0002 when introducing another backend library. A language-focus hint is descriptive and must not reject other input languages; return no detected language when the decoder cannot provide one. Check the license and attribution of each conversion. Haas-ja uses `oboroge0/Haas-ja-v1.0-onnx-quantized`, a community Apache-2.0 conversion of Third Intelligence's model, pinned at `7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39`.

For an actual sherpa ASR check, use `cargo run -p dito-runtimes --no-default-features --features sherpa --example asr_e2e -- /tmp/dito-asr-e2e haas-ja /path/to/bench/audio`. This downloads and verifies real weights, checks Japanese recognition, accepts other language hints, processes long audio, checks silence and exercises stop/restart without microphone access or text insertion.

The native sherpa binding is the official `sherpa-onnx-sys` 2.0.1 static package. Its build script downloads matching libraries automatically. Keep the binding and native version aligned; do not override `SHERPA_ONNX_LIB_DIR` with an older binary. The C ABI adapter reads the full result JSON because the upstream high-level Rust result type currently omits SenseVoice's language field. For model-page screenshots with a real running engine, set `DITO_MODEL_WARM=haas-ja` with `DITO_SHOTS_ONLY=models` and `DITO_MODEL_SHOTS=local-asr`; use an isolated `--data-dir` containing verified weights. No synthetic running status is created.

### Changing the config schema

Bump `SCHEMA_VERSION` in `crates/dito-core/src/config/migrate.rs` and append one migration function that rewrites the parsed TOML table from the previous version; migrations run in order before the typed load. Add one test per migration that feeds it a table in the old shape. Spell out the names and defaults a migration writes instead of reading them from live code (`asr_profile_ids` and `polish_configurations` are the pattern), so it keeps its result when a label or a template changes. Never edit or renumber a shipped migration, because files in the field have already passed through it. ADR-0015 lists the versions. Before rewriting, `ConfigStore::open` saves the original as `config.toml.v<old>.bak`, and it refuses a file with a newer version (ADR-0015).

### Adding a built-in writing style

Built-in styles (Slack, Teams, Herdr) are one table, `BUILTIN_STYLES` in `crates/dito-core/src/prompts.rs`. Add one entry with its `id`, `label`, the `bundle_ids` of the apps it applies to, and its default instruction and sample sentences in Japanese, English and Chinese. Style lookup by bundle id, the default prompt sets, the history filter and the settings pages all read the table. The `id` is the style's stored form (history rows and `disabled_builtin_styles`) and the key of its prompt in every saved prompt set, so never change it after release, and keep it different from `base`, `general`, `applications` and `application_defaults`.

The loader does not patch missing keys (ADR-0015), so a saved prompt set has no entry for the new style and would run it with an empty prompt. Add a config migration that inserts the style's default prompt into every saved language set that lacks it, as described in "Changing the config schema" above; `herdr_defaults` in `crates/dito-core/src/config/migrate.rs` is the pattern. Nothing else needs editing.
