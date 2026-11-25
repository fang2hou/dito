# Implementation

How to build, run, test and change Dito, for people and AI agents. Product behavior lives in [design/features.md](../design/features.md); structure lives in [ARCHITECTURE.md](ARCHITECTURE.md).

## Setup

| Tool | Purpose | Managed via |
| --- | --- | --- |
| Rust 1.99.1 with rustfmt and clippy | Compiler and checks | mise (`mise.toml`) |
| Python 3.15, uv, ruff and ty | The `bench/` benchmark project | mise (`mise.toml`); packages through uv |
| Apple Command Line Tools | macOS SDK and native compiler | `xcode-select --install` |
| Xcode (optional) | Liquid Glass app icon compilation and preview | Xcode |
| cmake | llama.cpp and sherpa-onnx builds | Homebrew |
| `dito-dev` signing identity | Stable permissions across rebuilds (see Engineering Traps) | `mise run certs` (also run by `release:install`), once per machine |

Cargo uses the selected macOS SDK and respects `DEVELOPER_DIR`. GPUI and llama.cpp compile their shaders at runtime with the pinned dependencies. The icon script uses a compatible Xcode for Liquid Glass when available and falls back to `Dito.icns` from the checked-in artwork.

## Commands

```bash
mise install                                  # tools at the pinned versions
mise run dev:mock                             # whole app with mocks (builds with the mock feature); never types into other apps
mise run dev                                  # real mode (real microphone, models and providers)
mise run release                              # optimized release bundle at target/bundle/Dito.app, launched
bash scripts/install-local.sh                 # build, sign and install to ~/Applications/Dito.app
mise run check                                # ./scripts/check.sh fast: fmt, unit tests, mock-server tests, i18n coverage
mise run lint:rust                            # clippy for the workspace, again with the mock and screenshots features
mise run lint:rust:ci                         # clippy for the CI crate set without local runtimes
mise run check:full                           # fast, plus compile checks of every build flavor
mise run check:design-refs                    # every design:<id> reference resolves
mise run lint                                 # lint:rust, ruff and ty for bench, actionlint for workflows
mise run hooks:install                        # once per clone: install the prek commit hooks (see CONTRIBUTING.md, Checks)
mise run hooks                                # run every hook against all files
mise run release:dmg                          # build the binary DMG
mise run readme:assets                        # regenerate the README animations (build the screenshot binary first)
mise run format                               # cargo fmt and ruff format
cargo test -p <crate>                         # tests for one crate
cargo test -p dito-app --no-default-features --test i18n_coverage  # after editing messages.toml
```

Extra arguments reach the app: `mise run dev:mock -- --data-dir <empty dir>`.

Cargo features of `dito-app`: `runtimes` (on-device inference, the only default; needs CMake), `mock` (the in-process mock cloud behind `--mock`, so release bundles do not link the test server) and `screenshots` (`--screenshots`; it includes `mock`, because the harness uses the mock cloud and its seed data). A flag whose feature is not compiled in fails with a usage error.

`mise run release` builds with `scripts/bundle.sh` (release profile with thin LTO and one codegen unit). `bash scripts/install-local.sh` (also available as `mise run release:install`) uses the current Rust toolchain and an optimized local profile with incremental compilation and parallel code generation. It creates or reuses the local signing identity, quits the running Dito release, replaces `~/Applications/Dito.app`, verifies the signature and opens the app. See [INSTALL.md](../INSTALL.md) for how the standalone installer prepares missing tools.

`mise run release:dmg` packs the bundle and an `/Applications` symlink into `target/dist/Dito-v<version>-macos-<arch>.dmg`. It uses `figcraft-cli` to export the DMG background at 1× and 2× from `design/dmg-background.figcraft`. The export hides the illustration layers (`app-icon` and `folder-*`) because Finder places the actual app and shortcut over the background. `design/dmg-layout.dsstore` stores the Finder window and icon positions and points to the background inside the volume. Update the layout alongside the design if the drop zones move.

The DMG and app are signed ad-hoc by default (set `CODESIGN_IDENTITY` to a Developer ID certificate to change that), so recipients clear Gatekeeper once via System Settings, Privacy & Security, Open Anyway.

The Python benchmark harness has its own tasks: `mise run bench:sync`, `bench:test`, `bench:lint`, `bench:format`, and `mise run bench <args>` for `dito-bench` (see `bench/README.md`).

## Workflow

1. Pick the matching workflow in [workflow](workflow/README.md): [adjust the UI](workflow/ui-adjustment.md) or [fix code](workflow/code-fix.md).
2. Read the ADRs that touch the change.
3. Make the smallest complete change.
4. Run `./scripts/check.sh fast` and `mise run lint:rust`; the commit hooks run both.
5. Commit following [CONTRIBUTING.md](CONTRIBUTING.md).

## Layout

- `crates/`: the workspace crates (see [ARCHITECTURE.md](ARCHITECTURE.md)).
- `design/`: `dito.figcraft` (UI and motion), `sections.toml` (the ids behind `design:<id>`), `features.md`, the cloud speech request JSON contract, and `asr-templates/` (compiled into the app).
- `assets/icon/`: the Icon Composer document.
- `bench/`: Python benchmark harness for ASR and polish models, with stored results.
- `mise.toml`: tool versions and the project tasks. `prek.toml`: commit hooks. `.github/`: PR rules, project checks and the release workflow (see [CONTRIBUTING.md](CONTRIBUTING.md)).
- `scripts/`: `bundle.sh` (release app), `install.sh` (install to `~/Applications`), `install-local.sh` (standalone source installer), `dmg.sh` (release DMG), `quit.sh` (quit the running release Dito), `certs.sh` (signing identity), `check.sh` (checks), `dev-runner.sh` (wraps runs in `DitoDev.app`), `icon.sh` (icon build and preview), `setup-macos.sh` (Metal toolchain), `readme-assets.py` (README animations), `check-design-icons.py` (geometry check of the saved design), `check-design-refs.py` (`design:<id>` references against `design/sections.toml`), `check-flow.sh` and `check-release-tag.sh` (branch and tag rules for CI), `workspace-version.sh` (the version in `Cargo.toml`, read by the bundle, DMG and tag scripts). `.agents/skills/` holds the project skills, including `icon-keylines`.
- `docs/`: this document, architecture, glossary, debugging, contributing and ADRs. `docs/workflow/`: step-by-step workflows.

## Coding Standards

- Code, comments and identifiers are English. UI copy comes only from `crates/dito-core/src/i18n/messages.toml`.
- Never join text fragments with `·`, `|`, `｜` or a separating `・`; write natural phrases.
- Keep platform code in `dito-platform` and logic in `dito-core`; the app wires them together.
- Every `unsafe` block states its safety conditions; AppKit calls stay on the main thread.
- Prefer a clearer ownership or type over a defensive check (see [fix code](workflow/code-fix.md)).
- gpui-kit component types (`gpui_kit::component::...`) are re-exported by `crates/dito-ui/src` and imported by the app through `crate::ui`; only that crate names those paths.

## Testing

- Tests live next to the code (unit) and in each crate's `tests/` (integration).
- Write a test when logic is easy to break and hard to notice in the app: parsing, config migration, state machines, insertion flow, i18n, protocols.
- Don't write defensive tests for layout, every error branch or hypothetical misuse. Look and motion are checked by a person in the running app.
- `check.sh` also runs `dito-app --test demo_run`: it needs a logged-in session but starts the app headless, so nothing appears on screen and focus stays where it is.
- Not run by `check.sh`, because they need the real Keychain, Accessibility or model files: `dito-platform --test keychain` and `--test loopback` (both `#[ignore]`, run with `-- --ignored`), and the ignored runtime regression tests (`DITO_ASR_MODEL_DIR`, `DITO_POLISH_MODEL_DIR`). `dito-runtimes` has no tests `check.sh` can run: its unit tests sit in the `llama` and `sherpa` modules, which need their features and the native builds.
- `check.sh` runs `dito-testkit --test bedrock` on a single thread, because those tests set AWS environment variables.

## Debugging

Flags, data locations, logs and a symptom table are in [DEBUGGING.md](DEBUGGING.md).

## Engineering Traps

Things that look removable or simplifiable but exist for a reason. Product-level judgments are in the [ADRs](adr/README.md).

- **Signing and the dev runner.** macOS ties Microphone and Accessibility grants to the code signature. `scripts/certs.sh` creates the `dito-dev` identity once; `.cargo/config.toml` routes runs through `scripts/dev-runner.sh`, which wraps the binary in `target/bundle/dev-<pid>/DitoDev.app` (one wrapper per run), signs it and launches it with `open`. A bare `target/debug/dito` gets no permission prompts and no accessibility answers, so never use it for permission or insertion work. `scripts/check.sh` bypasses the runner for automated tests.
- **Identifiers.** `com.fang2hou.dito` is the bundle id, signing identifier, Keychain service and login item label, and the data directory is `com.fang2hou.Dito`. Changing any of them resets permissions and orphans data and keys; change all of them together, with a migration plan, or not at all. They live in `dito-core/src/identity.rs` (and `scripts/bundle.sh`); debug builds take the `-dev` variants from there, and `scripts/dev-runner.sh` signs `DitoDev.app` with the same `com.fang2hou.dito-dev`.
- **Background frame rate.** The HUD is never key and Dito runs in the background, so macOS throttles it. While the HUD is open a `NapGuard` holds a user-interactive activity and the window opts out of inactive-window throttling (`inactive_frame_interval: None`); don't drop either without measuring an unfocused HUD. `HudView` advances its clock in next-frame callbacks on the window's display link. The clock asks for the next frame while the HUD listens or processes and while anything it times is moving, and parks once the HUD is still (a command window being read or typed in, a settled error), so the display link can pause. Every render arms a parked clock again, so whatever starts motion in the HUD must notify the view; a state whose motion starts without a notify belongs in `keeps_ticking` (`hud/view/clock.rs`). A new window draws once inside `open_window` and presents nothing until its first frame request, so the pop-in starts on the clock's first tick, not when the view is built or first rendered.
- **Listening ocean start.** The level model ignores the microphone for the first 120 ms of non-zero level while it measures the noise floor, and a recorder that has not delivered a buffer reports zero. Until then the water has only its quiet drift, about 0.2 pt of dot travel a second, which looks like a still picture. `START_MOTION` in `hud/waveform.rs` draws a small swell and ripple over the model for that time, fades it out over `blend_out_s` once the floor is known, and fades it after `longest_s` when no audio arrives, so an input that stays silent does not look alive. It is presentation only: the model never reads it, Reduce Motion drops it, the looping style preview does not use it, and once it has faded a frame equals the frame without it (`start_motion_shows_at_once_and_leaves_the_frames_of_real_audio_untouched`). Tune its values there. To time an opening on a real machine, see the `hud opening` line in [DEBUGGING.md](DEBUGGING.md).
- **Unload models on quit.** `LocalEngines::unload_all()` runs on quit because llama.cpp's Metal backend crashes at exit when a model is still loaded.
- **Cancel reaches local inference through a stop flag.** The pipeline cancels by dropping the future; the local engine wrappers then set the `stop` flag their runtime call was given. A runtime that ignores the flag keeps computing in `spawn_blocking` and holds the model's lock, so every new runtime loop must check it.
- **Outcome timing lives in two crates.** The controller's `SUCCESS_FLASH` and `COPIED_FLASH` must keep the window alive through the HUD's processing catch-up, feedback and exit in `hud/timing.rs`; synchronize them when changing either clock. Include the longest catch-up when calculating the window lifetime.
- **Pinned llama.cpp bindings.** `llama-cpp-2` and `llama-cpp-sys-2` are pinned to one exact version. `crates/dito-runtimes/src/readout.rs` calls two C++ functions of that llama.cpp tree by their mangled names and reads the raw model handle out of `llama-cpp-2`'s transparent wrapper. Upgrade both crates together and rerun `mise run bench intent parity`.
- **Intent heads belong to one model file and one prompt.** `crates/dito-core/src/intent/heads/*.heads` are compiled into `dito-core` and were fitted on the catalog's GGUF files with the text of `router_system.txt`. They are committed build inputs: a build never refits. After changing either, run `mise run bench intent all` and commit the new files; `cargo test -p dito-local --lib intent` fails until the heads are refitted. See [INTENT_ROUTING.md](INTENT_ROUTING.md).
- **Compiled-in design files.** `design/asr-templates/*.json` and `crates/dito-core/src/provider/asr-help/*.md` are compiled into `dito-core`. Moving or deleting them breaks the build. `design/asr-config.schema.json` is documentation only.
- **App icon.** `assets/icon/Dito.icon` is an Icon Composer document. `scripts/icon.sh compile` (release bundles) and `compile-cached` (dev wrappers) try to build `Assets.car` plus `Dito.icns`; when the selected Xcode cannot compile the layered icon they fall back to a flat `Dito.icns` from the checked-in 1024 px artwork, and `bundle.sh` / `dev-runner.sh` drop `CFBundleIconName` so the system draws the flat file. `preview` renders the appearances. Never call `setApplicationIconImage` in a bundled run (it flattens the Liquid Glass icon); only bare binaries set a PNG. Declare `features: ["refractivity", "specular-location"]` whenever those keys are used, and use `blur-material`, not `blur`, or Icon Composer won't open the file.
- **Dock icon policy.** With "Show icon in Dock" off, `main` starts GPUI with the Headless windowing request, which on macOS is only the Accessory activation policy (windows still draw). It must be set before the run loop; setting it later flashes the icon in the Dock at launch. `dock::sync` switches to Regular while the settings or onboarding window is open and back when the last one closes. The switch back runs one turn after the close callback: inside it, AppKit reports an active-status change for the removed window and GPUI logs `window not found`. Open a window, call `dock::sync`, then `cx.activate`; a Regular app that is not activated has no menu bar.
- **Mock mode types for real.** `--mock` without `--log-sink` types into the frontmost app. Agents and automated runs always add `--log-sink`.

## Known Limitations

- Cancelling local inference takes effect at the next llama.cpp token or the next sherpa-onnx chunk. A prompt evaluation, an audio encode or one sherpa-onnx decode that has already started runs to its end first.
- Bedrock polishing is covered only by the mock server, not by a real account.
- The warm-up of the cloud engines while a recording runs is verified only against the mock server on loopback: what a real provider answers to `HEAD /`, and how long it keeps the connection open afterwards, is not measured. A recording longer than the provider keeps an idle connection gains nothing, and a Bedrock engine is built by the warm-up but gets no open connection.
- A cloud request that reuses a pooled connection fails with a network error when the server or the network drops that connection in the same moment, and Retry then succeeds on a new one. Dictations in quick succession always reused connections; with the warm-up the first dictation after a pause does too.
- GitHub Actions runs the fast tests, CI clippy set and text checks on pull requests into `main` and pushes to `main`. It does not build the default-feature local runtimes; full runtime and release checks remain local.
- The display language defaults to the system language; the design calls for Japanese by default.

## Validation

The commit hooks run `./scripts/check.sh fast` and `mise run lint:rust`. The Checks workflow repeats the fast script, runs `mise run lint:rust:ci` and runs the text checks for every pull request into `main` and push to `main`; run `./scripts/check.sh full` before a release build. A green run proves the commands passed, not that the behavior is right; check UI changes in the running app.

### Changing the database schema

The SQLite store behind `dito.db` versions its schema with `PRAGMA user_version`, which equals the number of entries in `MIGRATIONS` (`crates/dito-core/src/store/schema.rs`) that have been applied. To change the schema, append a migration to that list; never edit or reorder one that has shipped, because existing databases have already run it. The store applies pending migrations when it opens the database and owns `user_version`, so nothing else writes it, and it refuses a database whose version is newer than the list. Add a test that opens a database at the previous version and checks that its rows survive and `user_version` reaches the new value.

### Adding a local model

The catalog in `dito-local` owns model files, pinned URLs, sizes, checksums, memory estimates, language-focus hints and the inference layout. Every Hugging Face URL must pin a 40-character commit SHA (never `main`), and `pages/models/v1.json` must list the same model files and URLs; unit tests verify both invariants. Pushing `pages/` to `main` publishes the static manifest to GitHub Pages (`https://fang2hou.github.io/dito/models/v1.json`), which `ModelManager` fetches on demand when a download starts to resolve replacement or mirror URLs while enforcing the compiled-in sizes and SHA-256 checksums. Set `DITO_MODEL_MANIFEST_URL` to override that endpoint during development, or empty to disable remote lookup. The Models page lists each kind from the smallest parameter count to the largest (`parameters_m`, the original model's count in millions), wherever the entry is written. For another checkpoint of an existing `ModelFormat`, add one catalog entry; downloads, verification, selection and engine lifecycle reuse the existing path. `ModelFormat` declares exact filenames for each role, so a projector or unrelated ONNX file cannot be mistaken for the main model. Keep external data files such as Haas-ja's `encoder.data` in the same download entry and directory.

Dictation aimed at one of Dito's own text fields (the chat message field, a Settings field) is pasted from inside the app (`services/own_window_sink.rs`): the platform sink only types into other apps, since Dito's windows publish no accessibility text. It applies when the Tools or Settings window is the key window and its focused element takes a paste; the user's clipboard goes back afterwards, and anything else falls through to the platform sink.

The chat tool sizes a request by where the text model runs (`Limits` in `dito-core/src/chat.rs`): a local model takes files up to 512 KB and about 128,000 characters of conversation, a cloud model 2 MB and 500,000. Its thinking pill lists what the model's `text_capabilities.thinking` declares, or low, medium and high for a cloud configuration, and the choice is saved per model in `chat.thinking`. Locally a choice opens or closes the think block at the end of the prompt, and a Qwen 3.8 level also writes the instruction its chat template would (`reasoning_instruction` in the catalog); a cloud level replaces `reasoning_effort` for that request. A reply that spends its whole budget thinking is reported as an error instead of showing the thoughts. The Local API takes the same choice as `reasoning_effort` on `/v1/chat/completions` and on the `generate_text` tool (`thinking_for` in `local_api/models.rs`): `none` switches thinking off, a cloud configuration gets the value as sent, a local model with levels refuses one it does not have, and the model listing names the accepted values as `reasoning_efforts`.

Local chat keeps the context of the last conversation for 10 minutes after its last use (`CHAT_SESSION_TTL` in `dito-runtimes/src/llama.rs`), so a follow-up reads only what is new instead of the whole conversation; one conversation is kept at a time, and another chat, an edited history or the expiry frees it. The Qwen 3.5 checkpoints keep recurrent state that cannot be rolled back, so the context holds a resident sequence that stops before the reply opens and each turn starts as a copy of it.

A chat model of another family (Gemma, Llama, Phi) is a catalog entry with `ModelFormat::Chat` too, and its prompt format comes from the chat template embedded in the GGUF, so a file without one fails to load. Pass `disable_thinking: false` for it (the flag is the Qwen hybrid-thinking prefill), give it the `sampling` the family recommends (the values the model uses for any parameter the user leaves empty on the Models page; `Sampling::POLISH` is what the Qwen entries use), and check its output handling (`strip_think`, sampling) against real weights before it ships.

A speech model published as one transcribe.cpp GGUF is a catalog entry with `ModelFormat::TranscribeCpp`, with no adapter work. Set `hotwords` from `Model::supports(Feature::Vocabulary)` on the real file. `transcribe-cpp` builds its native library from source with CMake, and `DITO_TRANSCRIBE_DEBUG=1` keeps its log on stderr. ADR-0002 states the rule for bumping it or `llama-cpp-sys-2`.

Nemotron 3.5 Transcribe uses `ModelFormat::NemoSpeech` on Apple Silicon. Its verified NeMo-Speech.cpp archive downloads with the encoder and decoder into the selected models directory. Startup unpacks it atomically, launches a private loopback server with a fresh authentication token, and waits for the model before publishing Running. Stop closes its owner pipe and waits for cleanup; the same pipe closes if Dito exits abruptly. No system-wide runtime installation is needed. Worker failures and process resource usage flow through the existing local model status. `nemo-speech.log` stays in that model's directory. Hotword injection is disabled because the measured vocabulary prompts can alter unrelated words. SALM consumes the complete recording with its native long context, not the 28-second small-model chunker. A varied 40-second synthetic recording was checked through the app pipeline. The 0.3.0 runtime can stop early on highly repetitive speech while still reporting `finish_reason=stop`; this remains an upstream recognition limitation.

Ternary Bonsai 2.5 27B (`PQ2_0`, 7.25 GB) uses `ModelFormat::PrismChat` on Apple Silicon through the same managed-worker path: its pinned `PrismML-Eng/llama.cpp` macOS arm64 archive downloads alongside the GGUF, unpacks atomically after SHA-256 verification, and launches a private authenticated `llama-server` on loopback (`prism-llama.log`). It supports polishing, streaming chat (`medium` and `xhigh` thinking levels, plus thinking off; `low` is excluded per the model card), and option-logprob command routing without bundled heads. This worker is temporary until upstream `llama-cpp-sys-2` and `transcribe-cpp` align on `llama.cpp >= b11556`, at which point the catalog entry can switch to embedded `ModelFormat::Chat`.

Development worktrees share `~/Library/Application Support/com.fang2hou.Dito-dev/models` by default. Keep `models_dir` pointed there when using an isolated `--data-dir`; do not copy weights into worktrees or benchmark directories. Model downloads use a file lock so concurrent instances reuse the completed files. To exercise real downloads, startup, transcription, stop and restart with synthetic bench audio, run `cargo run -p dito-runtimes --example managed_asr_e2e -- /tmp/dito-managed-asr bench/audio`. Its config is isolated, its weights use the development cache, and the `stop-loading` and `hold` modes cover startup cancellation and abrupt-owner-exit checks.

For a new architecture, add a `ModelFormat` variant and its adapter in `dito-runtimes`, declare its hotword capability, and extend the loader. Update ADR-0002 when introducing another backend library. A language-focus hint is descriptive and must not reject other input languages; return no detected language when the decoder cannot provide one. Check the license and attribution of each conversion. Haas-ja uses `oboroge0/Haas-ja-v1.0-onnx-quantized`, a community Apache-2.0 conversion of Third Intelligence's model, pinned at `7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39`.

For an actual sherpa ASR check, use `cargo run -p dito-runtimes --no-default-features --features sherpa --example asr_e2e -- /tmp/dito-asr-e2e haas-ja /path/to/bench/audio`. This downloads and verifies real weights, checks Japanese transcription, accepts other language hints, processes long audio, checks silence and exercises stop/restart without microphone access or text insertion.

The native sherpa binding is the official `sherpa-onnx-sys` 2.0.1 static package. Its build script downloads matching libraries automatically. Keep the binding and native version aligned; do not override `SHERPA_ONNX_LIB_DIR` with an older binary. The C ABI adapter reads the full result JSON because the upstream high-level Rust result type currently omits SenseVoice's language field. For model-page screenshots with a real running engine, set `DITO_MODEL_WARM=haas-ja` with `DITO_SHOTS_ONLY=models` and `DITO_MODEL_SHOTS=local-asr`; use an isolated `--data-dir` containing verified weights. No synthetic running status is created.

### Changing the config schema

Bump `SCHEMA_VERSION` in `crates/dito-core/src/config/migrate.rs` and append one migration function that rewrites the parsed TOML table from the previous version; migrations run in order before the typed load. Add one test per migration that feeds it a table in the old shape. Spell out the names and defaults a migration writes instead of reading them from live code (`asr_profile_ids` and `cloud_configurations` are the pattern), so it keeps its result when a label or a template changes. Never edit or renumber a shipped migration, because files in the field have already passed through it. ADR-0015 lists the versions. Before rewriting, `ConfigStore::open` saves the original as `config.toml.v<old>.bak`, and it refuses a file with a newer version (ADR-0015).

### Adding built-in app rules

Built-in app rules (Slack, Teams, Herdr) are one table, `BUILTIN_APP_RULES` in `crates/dito-core/src/prompts.rs`. Add one entry with its `id`, `label`, the `bundle_ids` of the apps it applies to, and its default instruction and sample sentences in Japanese, English and Chinese. Lookup by bundle id, the default prompt sets, the history filter and the settings pages all read the table. The `id` is the entry's stored form (history rows and `disabled_builtin_styles`) and the key of its prompt in every saved prompt set, so never change it after release, and keep it different from `base`, `general`, `applications` and `application_defaults`.

The loader does not patch missing keys (ADR-0015), so a saved prompt set has no entry for the new app rules and would run them with an empty prompt. Add a config migration that inserts their default prompt into every saved language set that lacks it, as described in "Changing the config schema" above; `herdr_defaults` in `crates/dito-core/src/config/migrate.rs` is the pattern. Nothing else needs editing.
