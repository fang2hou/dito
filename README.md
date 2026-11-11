<div align="center">

# Dito

Dictation for macOS: press a shortcut, speak, and polished text appears at your cursor.

</div>

Dito records your voice, turns it into text on your Mac or through a cloud provider, optionally polishes it with a language model, and types it into whatever app has focus. The interface is in Japanese, with English and Simplified Chinese available. Dito is in early, fast-moving development.

## 🚀 Usage / Quick Start

**With an AI coding agent**: paste this into the agent to hand it the repository.

```text
Work in this repository. Read AGENTS.md at the repository root first and follow it.
```

**As a user**:

1. Download the zipped app from the [latest release](../../releases/latest) and move `Dito.app` to `Applications`.
2. Open it. Dito starts in the menu bar and opens no window by itself; on the first launch it shows a short setup.
3. Allow **Microphone** and **Accessibility** when asked. Accessibility lets Dito type into other apps.
4. In **Models**, download a local model or add a cloud configuration.
5. Put the cursor in any text field, press the right **⌘** key, speak, and press it again. The text appears at the cursor.

| To | Do |
| --- | --- |
| Cancel while recording | Esc or the × on the HUD |
| Finish early | The ✓ on the HUD |
| Stop transcription or polishing | The Stop button on the HUD |
| Dictate while holding the key | Choose Hold mode in **General** |
| Change shortcut, language, microphone or appearance | **General** |
| Review or copy earlier results | **History** |
| Teach Dito your words | **Wordbook** |
| Change how text is polished per app | **Prompts** |

Recordings stop after 5 minutes and are still processed.

**From source**: needs macOS, [mise](https://mise.jdx.dev/), Xcode with its Metal Toolchain and cmake. Tool versions are pinned in `mise.toml`.

```bash
mise install
mise run dev:mock
```

The settings window opens and a menu bar icon appears, running against a mock cloud and a synthetic microphone. `mise run` lists every other task.

<details>
<summary>Permissions after an update</summary>

Release builds are ad-hoc signed, so macOS asks for both permissions again after each update. Building from source with your own signing identity avoids this; see [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md).

</details>

<details>
<summary>Where your data lives</summary>

Settings, history, recorded audio and downloaded models live in `~/Library/Application Support/com.fang2hou.Dito`. Cloud API keys are stored in the Keychain, never in files. Nothing is sent anywhere unless you choose a cloud provider for a stage.

</details>

## ✨ Features

- **Works in any app**: typing or paste with automatic fallback, and your clipboard restored afterwards.
- **Local or cloud, per stage**: on-device Qwen3-ASR and SenseVoice for speech, Qwen3.5 for polishing, or your own cloud configurations.
- **Polishing that follows the app**: separate writing styles for Slack, Teams and any app you add.
- **A HUD that stays out of the way**: a small capsule with a live waveform that never takes focus.
- **Never loses a dictation**: if polishing fails, the transcript is inserted; every result stays in History.
- **Private by default**: history and audio stay on your Mac, and API keys live in the macOS Keychain.

## 📚 Learn More

| Goal | Read |
| --- | --- |
| Build, run with mocks, test | [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md) |
| Run locally, flags, diagnose a problem | [docs/DEBUGGING.md](docs/DEBUGGING.md) |
| Architecture and decisions | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [ADRs](docs/adr/README.md) |
| Terms and where each concept lives | [docs/GLOSSARY.md](docs/GLOSSARY.md) |
| Contribute a change | [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) |
| What the app does and why | [design/features.md](design/features.md) |
| UI and motion design | [design/README.md](design/README.md) |
| Work with an AI coding agent | [AGENTS.md](AGENTS.md) |

## 📄 License

All rights reserved. See [LICENSE](LICENSE).
