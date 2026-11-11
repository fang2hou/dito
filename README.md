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

**As a human**: requires macOS, [mise](https://mise.jdx.dev) (installs the pinned Rust and Python tools from `mise.toml`), Xcode with its Metal Toolchain, and cmake.

```bash
mise install          # pinned Rust, Python, uv and ruff
mise run certs        # once per machine: signing identity that keeps permissions across rebuilds
mise run dev:mock     # the whole app with mock providers; nothing is typed into other apps
```

The settings window opens and a menu bar icon appears. Put the cursor in a text field and press right ⌘ to start and stop a dictation. Dito needs Microphone and Accessibility permissions; the settings window links to both.

<details>
<summary>Real mode and release builds</summary>

- `mise run dev` uses the real microphone, local models and providers. Download local models under Models → Local, or set up a provider under Models.
- `mise run release` builds the optimized `target/bundle/Dito.app` and launches it.
- `mise run release:install` builds the release bundle and installs it to `~/Applications`. It creates the `dito-dev` signing identity first when it is missing (one macOS authorization prompt).
- Full Xcode is needed for the Metal Toolchain (`mise run setup:macos`, once) and the Liquid Glass icon. Release bundles and the icon step find it through `xcode-select` or `/Applications/Xcode.app`; `mise run dev` needs `DEVELOPER_DIR` exported (for example `/Applications/Xcode.app/Contents/Developer`) when only the Command Line Tools are selected. When the selected Xcode cannot compile the layered icon, builds fall back to a flat icon and print a warning.

</details>

## ✨ Features

- **Works in any app**: verified typing or paste with automatic fallback, and the clipboard restored afterwards.
- **Local or cloud, per stage**: on-device Qwen3-ASR, SenseVoice and Qwen3.5, or cloud configurations through an editable request JSON.
- **Polishing that follows the app**: prompts per language, with General, Slack and Teams styles.
- **A HUD that never steals focus**: a small capsule with a live waveform, progress and clear errors.
- **Never loses a dictation**: polish failures insert the raw text, and every result stays in History.

## 📚 Learn More

| Goal | Read |
| --- | --- |
| What the app does and why | [design/features.md](design/features.md) |
| UI and motion design | [design/README.md](design/README.md) |
| Architecture and decisions | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [ADRs](docs/adr/README.md) |
| Build, test and debug | [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md) |
| Contribute a change | [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) |
| Give it to an agent | [AGENTS.md](AGENTS.md) |

## 📄 License

All rights reserved. See [LICENSE](LICENSE).
