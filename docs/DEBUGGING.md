# Debugging

Run, inspect and diagnose Dito locally. Setup and commands are in [IMPLEMENTATION.md](IMPLEMENTATION.md); terms are in [GLOSSARY.md](GLOSSARY.md).

## Safe defaults

- Humans: `mise run dev:mock` runs the whole app against a mock cloud and a synthetic microphone.
- Agents and automated runs: always `--mock --log-sink`. `--mock` alone types into the frontmost app.
- Never use a bare `target/debug/dito` for permission or insertion work; use `mise run dev`, which signs and launches `DitoDev.app`. Every run has its own copy of that wrapper, so several dev instances can run at once and closing one leaves the others open.

## Flags, data and logs

| Flag | Effect |
| --- | --- |
| `--mock` | Mock cloud server, memory secrets, synthetic microphone, data in `$TMPDIR/dito-mock` (build with `--features mock`) |
| `--log-sink` | Log results instead of typing them |
| `--real-mic` | Use the real microphone in mock mode |
| `--wav <file>` | Use a WAV file as the recording |
| `--data-dir <dir>` | Config and history location; an empty directory shows onboarding |
| `--lang <tag>` | Pretend the system language is `ja`, `en`, `zh`, `zh-Hant`, `ko`, `mn` or `fr` |
| `--open-settings` | Open the settings window at launch. A launch opens no window by itself (see [ADR 0014](adr/0014-silent-launch.md)), so `mise run dev` and `dev:mock` pass this flag |
| `--demo-run` | Run one dictation headlessly and print it; no Dock icon, menu bar or window, never frontmost |
| `--screenshots <dir>` | Render screenshots (build with `--features screenshots`; narrow with `DITO_SHOTS_ONLY`); windows are never shown and never take focus |

- `DITO_SHOTS_ONLY=hud,demo` renders only the named shot groups; without it the default groups render and the opt-in ones stay off. Every group is declared once, in `GROUPS` in `crates/dito-app/src/dev/shots/runner.rs`. A name that is not declared there, or a value that names no group, ends the run with an error and the list of valid names before anything is rendered, so a mistyped name cannot pass as a run that rendered nothing.
- The first-run intro film (see `design:onboarding`) plays before the welcome page of a wizard that opens by itself, once per data directory (a `intro-seen` file; mock runs keep none, and automated runs never play it). Hold Ctrl to play it three times faster, press Esc to skip it. `DITO_INTRO=always` plays it again and `DITO_INTRO=never` suppresses it; the dev runner passes both on, and points `DITO_INTRO_DIR` at `assets/intro` because the dev wrapper bundle carries no films. Mock runs open no wizard (they are onboarded), so test with a real run: `DITO_INTRO=always cargo run -p dito-app -- --data-dir /tmp/dito-fresh`.
- Logs go to stdout through `tracing`; the dev runner tails the app's output into the terminal. Add directives with `RUST_LOG`; the dev runner hands it to the app (`open` starts it with a clean environment otherwise). The Status page shows the same lines of the app (the `dito`, `dito_audio` and `dito_trace` targets) since launch, see the next section.
- Data lives in `~/Library/Application Support/com.fang2hou.Dito`: `config.toml`, `dito.db`, `audio/` and `models/`. Keys are in the Keychain under `com.fang2hou.dito`, one item per cloud polish configuration (`polish.<id>`) and per cloud speech configuration (`asr.<id>`), and one for the Local API access token (`local_api.token`). The login item is `~/Library/LaunchAgents/com.fang2hou.dito.plist`.
- Debug builds (`mise run dev`, `cargo run`, tests) use a separate identity, `com.fang2hou.dito-dev` (data in `com.fang2hou.Dito-dev`, own Keychain items, login item, and Microphone and Accessibility grants), so they never read or change the installed app's config, history, keys or login item. API keys, models and permissions start empty there; point `models_dir` at the release folder to reuse downloaded models.
- `--data-dir` doesn't move the models folder; set `models_dir` for that.
- Use `--mock --log-sink` for any automated or agent run.
- Examples for manual checks: `cargo run -p dito-audio --example record`, `cargo run -p dito-platform --example probe`, `cargo run -p dito-platform --example press_shortcut -- 2` (presses right Command, to drive a running Dito), `cargo run -p dito-platform --example request -- accessibility` or `-- mic` (asks for a permission and watches its status), `cargo run -p dito-runtimes --example local_e2e`. The `intent_decide` and `intent_probe` examples of `dito-runtimes` are driven by `mise run bench intent` (see `bench/INTENT.md`).

## Debug log of a run

For the testing phase, every dictation and command can be followed step by step. The lines go to one target, `dito_trace`, at debug level. It is off by default and independent of every other target, so it is switched on by itself; the other defaults (`dito=info`, `dito_audio=debug`) stay as they are. There are two ways to switch it on:

- In the app: the Status page ends with a Log group, a plain monospaced view of the app's log lines since launch, one line per event: time, level, run id (`-` when the line belongs to no run) and the message with its fields. It scrolls, follows new lines while it is at the bottom, and its text can be selected; the copy button puts every line on the clipboard. The Detailed log switch turns the `dito_trace` lines on and off at once, in the view and in the terminal, without a restart. It is off at launch and is not saved: it lives for the session, because the detailed log holds full texts. Lines recorded while it was on stay in the view after it is turned off.
- From the terminal: `RUST_LOG=dito_trace=debug` starts the app with the switch on, and the switch can then turn it off:

```bash
RUST_LOG=dito_trace=debug mise run dev:mock
```

The view keeps the newest 1 000 lines and drops the oldest. It lives in memory only: nothing is written to disk or sent anywhere, and it ends with the app. A line is shown only when it passes the same filter as the terminal output, and only for the app's own targets (those starting with `dito`); the default levels are `dito=info` and `dito_audio=debug`. A run's lines carry its id only while the switch is on, because the id comes from the span that the `dito_trace` target opens.

Every line of a run carries `run=<id>` (`d` for a dictation, `c` for a command, then six hex digits), so `grep 'run=c3a91f2'` on the output follows one run from the shortcut to the delivery. A run logs:

- the shortcut press and release with the run kind, recording start and stop (duration, level and the silence verdict) and, for a command, the length and a short preview of the captured selection;
- the speech recognition request and its transcript, with timings;
- the router request (the instruction) and the full decision: every intent's probability, the confidence, whether the gate passed and the rewrite operation;
- which tool was chosen and why (confident, chosen by the user, polish button) and the window that asks for text, a choice or a confirmation, with the user's answer;
- the full system prompt and user message sent to the text model for every tool and for a dictation polish, the model's output and how long each step took;
- how the result was delivered, and the history row that was queued;
- model load and unload, and model downloads (start, progress in steps of ten percent, finish or failure), which belong to a model and carry `model=<id>` instead of a run id.

Full texts appear only in this log, and only at debug level. Keys and audio are never logged. The default `dito=info` output is unchanged. To follow a run in the view, switch the Detailed log on first and read the lines that share the run id.

## Symptom to cause

| Symptom | Likely cause | Look at |
| --- | --- | --- |
| No permission prompt, or Accessibility never answers | Running the bare binary, or the signing identity changed | Engineering Traps, signing, in [IMPLEMENTATION.md](IMPLEMENTATION.md); `mise run certs` |
| Permissions reset after a rebuild | The build was signed with a different identity | `mise run release:install` signs with `dito-dev` |
| Crash on quit with a local model loaded | `LocalEngines::unload_all()` was skipped | `dito-local`, llama.cpp Metal backend |
| The ocean starts late or looks still when the HUD opens | The first frames came late (`waited`, `longest_gap`), or the microphone delivered its first buffer late (`first_level`), so the start motion ran longer before the real level took over (`calibrated`) | `RUST_LOG=dito::hud=debug` prints one `hud opening` line a second after each HUD opens (none for a shorter recording); `START_MOTION` in `hud/waveform.rs` |
| HUD animation stutters while unfocused | macOS throttles a background app; `NapGuard` or the HUD's own timer was removed | `hud/`, `NapGuard` |
| Cancel leaves the CPU busy | A local runtime loop ignores its `stop` flag | `dito-runtimes` loops, `spawn_blocking` |
| Build fails after moving a file under `design/` or `provider/asr-help/` | Those files are compiled into `dito-core` | `design/asr-templates/`, `crates/dito-core/src/provider/asr-help/` |
| Startup window instead of the app | `config.toml` or `dito.db` is unreadable or newer than this build | `design:settings/startup-failure`; the file path is shown in the window |
| `config.toml` rewritten on launch | A schema migration ran; the original is saved as `config.toml.v<old>.bak` | `crates/dito-core/src/config/migrate.rs` |
| `mise run check:design-refs` reports an unknown design id | A `design:<id>` reference has a typo, or its id is missing from `design/sections.toml` | The reported file and line, `design/sections.toml` |
| `mise run check:design-refs` reports a layer that is not found | The section was renamed, renumbered or removed in the design | `board`, `item` and `layer` of that id in `design/sections.toml` |
