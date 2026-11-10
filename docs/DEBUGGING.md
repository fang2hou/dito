# Debugging

Run, inspect and diagnose Dito locally. Setup and commands are in [IMPLEMENTATION.md](IMPLEMENTATION.md); terms are in [GLOSSARY.md](GLOSSARY.md).

## Safe defaults

- Humans: `mise run dev:mock` runs the whole app against a mock cloud and a synthetic microphone.
- Agents and automated runs: always `--mock --log-sink`. `--mock` alone types into the frontmost app.
- Never use a bare `target/debug/dito` for permission or insertion work; use `mise run dev`, which signs and launches `DitoDev.app`.

## Flags, data and logs

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
- Data lives in `~/Library/Application Support/com.fang2hou.Dito`: `config.toml`, `dito.db`, `audio/` and `models/`. Keys are in the Keychain under `com.fang2hou.dito`, one item per cloud polish configuration (`polish.<id>`) and per cloud speech configuration (`asr.<id>`). The login item is `~/Library/LaunchAgents/com.fang2hou.dito.plist`.
- Debug builds (`mise run dev`, `cargo run`, tests) use a separate identity, `com.fang2hou.dito-dev` (data in `com.fang2hou.Dito-dev`, own Keychain items, login item, and Microphone and Accessibility grants), so they never read or change the installed app's config, history, keys or login item. API keys, models and permissions start empty there; point `models_dir` at the release folder to reuse downloaded models.
- `--data-dir` doesn't move the models folder; set `models_dir` for that.
- Use `--mock --log-sink` for any automated or agent run.
- Examples for manual checks: `cargo run -p dito-audio --example record`, `cargo run -p dito-platform --example probe`, `cargo run -p dito-platform --example press_shortcut -- 2` (presses right Command, to drive a running Dito), `cargo run -p dito-platform --example request -- accessibility` or `-- mic` (asks for a permission and watches its status), `cargo run -p dito-runtimes --example local_e2e`.

## Symptom to cause

| Symptom | Likely cause | Look at |
| --- | --- | --- |
| No permission prompt, or Accessibility never answers | Running the bare binary, or the signing identity changed | Engineering Traps, signing, in [IMPLEMENTATION.md](IMPLEMENTATION.md); `mise run certs` |
| Permissions reset after a rebuild | The build was signed with a different identity | `mise run release:install` signs with `dito-dev` |
| Crash on quit with a local model loaded | `LocalEngines::unload_all()` was skipped | `dito-local`, llama.cpp Metal backend |
| HUD animation stutters while unfocused | macOS throttles a background app; `NapGuard` or the HUD's own timer was removed | `hud/`, `NapGuard` |
| Cancel leaves the CPU busy | A local runtime loop ignores its `stop` flag | `dito-runtimes` loops, `spawn_blocking` |
| Build fails after moving a file under `design/` or `provider/asr-help/` | Those files are compiled into `dito-core` | `design/asr-templates/`, `crates/dito-core/src/provider/asr-help/` |
| Startup window instead of the app | `config.toml` or `dito.db` is unreadable or newer than this build | `design:settings/startup-failure`; the file path is shown in the window |
| `config.toml` rewritten on launch | A schema migration ran; the original is saved as `config.toml.v<old>.bak` | `crates/dito-core/src/config/migrate.rs` |
| `scripts/check-design-refs.py` reports an unknown id | A section was renamed or removed in the design | `design/sections.toml` |
