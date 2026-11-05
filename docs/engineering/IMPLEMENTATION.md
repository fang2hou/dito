# Implementation

How to build, run, test and change Dito, for people and AI agents. Product behavior lives in [design/features.md](../../design/features.md); structure lives in [ARCHITECTURE.md](ARCHITECTURE.md).

## Setup

| Tool | Purpose | Managed via |
| --- | --- | --- |
| Rust 1.99.1 with rustfmt and clippy | Compiler and checks | `rust-toolchain.toml` |
| Xcode and its Metal Toolchain | GPUI shaders, `actool` and `ictool` for the icon | Xcode (`xcodebuild -downloadComponent MetalToolchain`) |
| cmake | llama.cpp and sherpa-onnx builds | Homebrew |
| `dito-dev` signing identity | Stable permissions across rebuilds (see Engineering Traps) | `./scripts/certs.sh`, once per machine |

If only the Command Line Tools are selected, export `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`.

## Commands

```bash
cargo run -p dito-app -- --mock --log-sink    # whole app with mocks; never types into other apps
cargo run -p dito-app                         # real mode (real microphone, models and providers)
./scripts/check.sh fast                       # fmt, unit tests, mock-server tests, i18n coverage
./scripts/check.sh full                       # fast, plus compile checks of every build flavor
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p <crate>                         # tests for one crate
cargo test -p dito-app --test i18n_coverage   # after editing messages.toml
./scripts/bundle.sh                           # release build at target/bundle/Dito.app
```

Install a release build by quitting Dito, replacing `~/Applications/Dito.app` with `target/bundle/Dito.app`, and opening it.

## Workflow

1. Pick the matching workflow in [docs/workflow](../workflow/README.md): [adjust the UI](../workflow/ui-adjustment.md) or [fix code](../workflow/code-fix.md).
2. Read the ADRs that touch the change.
3. Make the smallest complete change.
4. Run `./scripts/check.sh fast` and clippy.
5. Commit following [CONTRIBUTING.md](CONTRIBUTING.md).

## Layout

- `crates/`: the workspace crates (see [ARCHITECTURE.md](ARCHITECTURE.md)).
- `design/`: `dito.figcraft` (UI and motion), `features.md`, the ASR connection config contract, and `asr-templates/` (compiled into the app).
- `assets/icon/`: the Icon Composer document.
- `scripts/`: `bundle.sh` (release app), `certs.sh` (signing identity), `check.sh` (checks), `dev-runner.sh` (wraps runs in `DitoDev.app`), `icon.sh` (icon build and preview).
- `docs/engineering/`: this document, architecture, contributing and ADRs. `docs/workflow/`: step-by-step workflows.

## Coding Standards

- Code, comments and identifiers are English. UI copy comes only from `crates/dito-core/src/i18n/messages.toml`.
- Never join text fragments with `·`, `|`, `｜` or a separating `・`; write natural phrases.
- Keep platform code in `dito-platform` and logic in `dito-core`; the app wires them together.
- Every `unsafe` block states its safety conditions; AppKit calls stay on the main thread.
- Prefer a clearer ownership or type over a defensive check (see [fix code](../workflow/code-fix.md)).

## Testing

- Tests live next to the code (unit) and in each crate's `tests/` (integration).
- Write a test when logic is easy to break and hard to notice in the app: parsing, config migration, state machines, insertion flow, i18n, protocols.
- Don't write defensive tests for layout, every error branch or hypothetical misuse. Look and motion are checked by a person in the running app.
- Not run by `check.sh`, because they need a GUI session, the real Keychain, Accessibility or model files: `dito-app --test demo_run`, `dito-platform --test keychain`, `dito-platform --test loopback`, `dito-testkit --test configured_asr`, and the ignored runtime regression tests (`DITO_ASR_MODEL_DIR`, `DITO_POLISH_MODEL_DIR`).

## Debugging

| Flag | Effect |
| --- | --- |
| `--mock` | Mock cloud server, memory secrets, synthetic microphone, data in `$TMPDIR/dito-mock` |
| `--log-sink` | Log results instead of typing them |
| `--real-mic` | Use the real microphone in mock mode |
| `--wav <file>` | Use a WAV file as the recording |
| `--data-dir <dir>` | Config and history location; an empty directory shows onboarding |
| `--lang <code>` | Pretend the system language is `ja`, `en` or `zh` |
| `--background` | Start without the settings window |
| `--demo-run` | Run one dictation headlessly and print it |
| `--screenshots <dir>` | Render screenshots (build with `--features screenshots`; narrow with `DITO_SHOTS_ONLY`) |

- Logs go to stdout through `tracing`; the dev runner tails the app's output into the terminal. Override with `RUST_LOG`.
- Data lives in `~/Library/Application Support/com.fang2hou.Dito`: `config.toml`, `dito.db`, `audio/` and `models/`. Keys are in the Keychain under `com.fang2hou.dito`. The login item is `~/Library/LaunchAgents/com.fang2hou.dito.plist`.
- `--data-dir` doesn't move the models folder; set `models_dir` for that.
- Use `--mock --log-sink` for any automated or agent run.
- Examples for manual checks: `cargo run -p dito-audio --example record`, `cargo run -p dito-platform --example probe`, `cargo run -p dito-runtimes --example local_e2e`.

## Engineering Traps

Things that look removable or simplifiable but exist for a reason. Product-level judgments are in the [ADRs](adr/README.md).

- **Signing and the dev runner.** macOS ties Microphone and Accessibility grants to the code signature. `scripts/certs.sh` creates the `dito-dev` identity once; `.cargo/config.toml` routes runs through `scripts/dev-runner.sh`, which wraps the binary in `target/bundle/DitoDev.app`, signs it and launches it with `open`. A bare `target/debug/dito` gets no permission prompts and no accessibility answers, so never use it for permission or insertion work. `scripts/check.sh` bypasses the runner for automated tests.
- **Identifiers.** `com.fang2hou.dito` is the bundle id, signing identifier, Keychain service and login item label, and the data directory is `com.fang2hou.Dito`. Changing any of them resets permissions and orphans data and keys; change all of them together, with a migration plan, or not at all.
- **Background frame rate.** The HUD is never key and Dito runs in the background, so macOS throttles it. While the HUD is open a `NapGuard` holds a user-interactive activity, and `HudView` drives its own 16 ms timer. Don't replace either with frame callbacks without measuring an unfocused HUD.
- **Unload models on quit.** `LocalEngines::unload_all()` runs on quit because llama.cpp's Metal backend crashes at exit when a model is still loaded.
- **Cancel means stop waiting.** The pipeline's cancellation stops waiting for local inference; the computation in `spawn_blocking` runs to completion and its result is dropped.
- **Outcome timing lives in two crates.** The controller's `SUCCESS_FLASH` and `COPIED_FLASH` must equal the HUD's `OUTCOME_END` and `COPY_END` in `hud/drawing.rs`; change both together.
- **Compiled-in design files.** `design/asr-templates/*.json` and `crates/dito-core/src/provider/asr-help/*.md` are compiled into `dito-core`. Moving or deleting them breaks the build. `design/asr-config.schema.json` is documentation only.
- **App icon.** `assets/icon/Dito.icon` is an Icon Composer document. `scripts/icon.sh compile` builds `Assets.car` and a flat `Dito.icns`; `preview` renders the appearances. Never call `setApplicationIconImage` in a bundled run (it flattens the Liquid Glass icon); only bare binaries set a PNG. Declare `features: ["refractivity", "specular-location"]` whenever those keys are used, and use `blur-material`, not `blur`, or Icon Composer won't open the file.
- **Mock mode types for real.** `--mock` without `--log-sink` types into the frontmost app. Agents and automated runs always add `--log-sink`.

## Known Limitations

- Cancelling local inference stops waiting, not the computation.
- The login item is a LaunchAgent and starts the app without `--background`.
- Bedrock and the legacy cloud ASR paths are covered only by the mock server, not by a real account.
- There is no CI; `./scripts/check.sh` is the gate.
- The display language defaults to the system language; the design calls for Japanese by default.

## Validation

`./scripts/check.sh fast` before every commit, `./scripts/check.sh full` before a release build, clippy always. A green run proves the commands passed, not that the behavior is right; check UI changes in the running app.
