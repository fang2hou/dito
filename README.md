<div align="center">

# Dito

Dictation for macOS: press a shortcut, speak, and polished text appears at your cursor.

</div>

Dito records your voice, recognizes it on your Mac or through a cloud provider, optionally polishes it with a language model, and types it into whatever app has focus. The UI is in Japanese, with English and Simplified Chinese available. It is in early, fast-moving development.

## 🚀 Quick Start

**With an AI coding agent**: paste this into the agent to hand it the repository:

```text
Work in this repository. Read AGENTS.md at the repository root first and follow it.
```

**As a human**: requires macOS, Rust 1.99.1 (pinned in `rust-toolchain.toml`), Xcode with its Metal Toolchain, and cmake.

```bash
./scripts/certs.sh                          # once per machine: signing identity that keeps permissions across rebuilds
cargo run -p dito-app -- --mock --log-sink  # the whole app with mock providers; nothing is typed into other apps
```

The settings window opens and a menu bar icon appears. Put the cursor in a text field and press right ⌘ to start and stop a dictation. Dito needs Microphone and Accessibility permissions; the settings window links to both.

<details>
<summary>Real mode and release builds</summary>

- `cargo run -p dito-app` uses the real microphone, local models and providers. Download local models under Models → Local, or set up a provider under Models.
- `./scripts/bundle.sh` builds `target/bundle/Dito.app`.
- If only the Command Line Tools are selected, export `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` and run `xcodebuild -downloadComponent MetalToolchain` once.

</details>

## ✨ Features

- **Works in any app**: verified typing or paste with automatic fallback, and the clipboard restored afterwards.
- **Local or cloud, per stage**: on-device Qwen3-ASR, SenseVoice and Qwen3.5, or cloud presets through an editable JSON connection config.
- **Polishing that follows the app**: prompts per language, with General, Slack and Teams styles.
- **A HUD that never steals focus**: a small capsule with a live waveform, progress and clear errors.
- **Never loses a dictation**: polish failures insert the raw text, and every result stays in History.

## 📚 Learn More

| Goal | Read |
| --- | --- |
| What the app does and why | [design/features.md](design/features.md) |
| UI and motion design | [design/README.md](design/README.md) |
| Architecture and decisions | [docs/engineering/ARCHITECTURE.md](docs/engineering/ARCHITECTURE.md), [ADRs](docs/engineering/adr/README.md) |
| Build, test and debug | [docs/engineering/IMPLEMENTATION.md](docs/engineering/IMPLEMENTATION.md) |
| Contribute a change | [docs/engineering/CONTRIBUTING.md](docs/engineering/CONTRIBUTING.md) |
| Give it to an agent | [AGENTS.md](AGENTS.md) |

## 📄 License

All rights reserved. See [LICENSE](LICENSE).
