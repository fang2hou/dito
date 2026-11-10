# Dito: Goals and Features

What the app is for and what it does, from the user's side. How it looks and moves is in `dito.figcraft` (conventions in [README.md](README.md)); how it is built is in [docs](../docs/README.md).

## Goals

- **Speak instead of type, anywhere.** One shortcut, then talk; polished text appears where the cursor is, in any app.
- **Never lose what was said.** If polishing fails, the raw text is inserted. If there is nowhere to type, the text goes to the clipboard. Every result stays in History.
- **Stay out of the way.** The HUD never takes focus, never covers menus, and disappears when done. Starting Dito opens no window. Settings change in one place each.
- **Local first, cloud when wanted.** Recognition and polishing each run on the Mac or through a provider, chosen separately.
- **Japanese first.** The UI is designed in Japanese; English and Simplified Chinese are available. The speech language is a separate setting.
- **Calm and precise.** Short, purposeful motion; no flashing; Reduce Motion respected everywhere.

## Dictation

- Press the shortcut (right ⌘ alone by default), speak, and the text is recognized, optionally polished, and typed at the cursor.
- **Toggle mode** (default): press to start, press again to finish. **Hold mode**: speak while holding, release to finish.
- Esc or × cancels while recording; ✓ finishes; Stop cancels recognition or polishing.
- A lone-modifier shortcut followed quickly by another key counts as typing, not as a dictation.
- Recordings stop at 5 minutes; a countdown appears in the last minute.
- Insertion uses key input (default) or paste, with automatic fallback, and restores the clipboard afterwards.
- Other audio is muted while the microphone is live (on by default).
- Silent or very short clips are discarded quietly.
- The polishing style follows the app in front: installed Slack, Teams and Herdr presets, custom app profiles, or Other.

## HUD

- A small capsule above the Dock, or following the pointer. It never takes focus and stays above app windows.
- States: listening (with a live waveform), recognizing, polishing, inserted, copied, and errors (authentication, no speech, microphone, network, other).
- Errors explain the reason and offer Retry or Open Settings; they close after 3 seconds, and hovering keeps them open.
- Hovering shows the speech language selector, button tooltips and the Stop button.
- Its appearance (system, light or dark; dark by default) is set separately from the settings window.

## Settings

| Page | What it does |
| --- | --- |
| 一般 (General) | Shortcut, record mode, insert method, music mute, microphone (automatic or fixed), speech language, display language, appearance, HUD appearance and position, launch at login, restart onboarding |
| モデル (Models) | Local models (download, pause, resume, verify, delete, switch with 使用する, move the folder), with request parameters per local polish model; cloud configurations for speech and for polishing, shown as a scrolling row of tiles that can be added from a template (FunASR, Qwen ASR, OpenRouter for speech; Azure OpenAI, Amazon Bedrock, OpenAI-compatible for polishing), duplicated, renamed, reordered by dragging and deleted; cloud speech edits the full request in the inline JSON editor with valid-draft autosave, native undo and redo, and a single documentation help dialog with four scrollable reference cards; cloud polishing has request parameters (temperature, top-p, max tokens, reasoning effort, extra JSON); connection tests |
| 辞書 (Dictionary) | Words passed to recognition as hotwords, optionally also to polishing; import and export; a banner when an action fails |
| プロンプト (Prompts) | Shared rules and app writing profiles per language, installed-app icons, add or remove an app profile, copy settings to another app, optional style examples and a safe preview. `design:settings/prompts`: taller editors, grouped undo and redo, draft autosave, scoped reset confirmation and plain-text before and after panes |
| 履歴 (History) | Search and filter by period, text or app; entries from configured apps carry the app name and the rest read as Other; each entry shows the polished text and the original; re-recognize, re-polish, copy, play audio, delete |
| 統計 (Stats) | Characters, speaking speed, time saved, recordings, top apps and languages |
| ステータス (Status) | Engine state, connection tests, macOS permissions (Microphone and Accessibility) |

The sidebar shows a health dot (green when ready, red with the reason when a local model is stopped) and collapses to a rail in narrow windows.

## Models

- Local recognition: Qwen3-ASR 1.7B (default, most accurate) and 0.6B, both with hotwords; SenseVoice Small (fastest).
- Local polishing: Qwen3.5 Instruct 2B, 4B and 9B.
- Nothing downloads until the user asks; downloads resume and are verified. Models in use start at launch.

## Menu Bar

A waveform icon showing the engine status and today's character count, with shortcuts to History, Dictionary, Status, Settings and Quit. It never changes settings.

## Dock

- **Dock icon (General, on by default).** Off, Dito runs from the menu bar alone and has no Dock tile. While a window is open the tile comes back, so the window can be found with the Dock and Cmd-Tab, and it leaves again when the last window closes.
- **Dock menu.** Right-clicking the tile lists the first four rows of the menu bar menu, Settings first, with no status line. Like the menu bar, it only opens pages.
- **Reopen.** Clicking the tile, or opening Dito again from Finder or Spotlight, while it runs with no window opens Settings on General.

## Onboarding

Shown on first launch and restartable from General, in seven steps: welcome, permissions (Microphone and Accessibility), how to use (with a looping demo), recording shortcut, engine choice (local or provider), preparing the models or the connection, and trying a dictation. Steps slide forward and back; with Reduce Motion they cross-fade.

## Behavior Details

Design decisions that aren't visible in a single screen. Timings and sizes themselves live in `dito.figcraft`.

- **Recording limit.** Recordings stop at 5 minutes and are processed, never discarded. The countdown appears at 4:00, in its own reserved space so the HUD never moves, and turns amber for the last 10 seconds.
- **Music mute.** Other audio is muted only while the microphone is live, and only if something is playing and the output isn't already muted. When recording stops, the previous state is restored, not simply unmuted.
- **Automatic microphone.** Automatic picking opens only wired devices (built-in, USB, Thunderbolt and similar). Opening a Bluetooth headset drops it to its low-quality call profile, and opening an iPhone wakes it as a Continuity microphone, so those are chosen by hand.
- **HUD size.** The capsule is 105 × 33 pt (72 pt wide in hold mode while listening), drawn at 1× in whole points, with opaque surfaces and one soft shadow.
- **Outcome timing.** The HUD hides when its outcome animation ends: about 1.1 s after Inserted and 1.9 s after Copied. Errors stay 3 s, and hovering keeps them open.

- **Silent launch.** Starting Dito, at login or by hand, means the user wants to dictate, not to configure. No window opens, the app never takes focus, and the menu bar icon is the sign that it runs. Three things may open a window by themselves: Onboarding on first run, Settings when a permission is missing, because dictation cannot work without it (General when Accessibility is missing, where the banner explains it; Status when only the Microphone is denied), and the startup failure window when the settings or data cannot be read, which shows the file and the reason and can restore default settings. Otherwise Settings open on request: menu bar, Dock menu, Dock click, or opening the app again while it runs.

## Known Gaps

- The display language follows the system by default; the goal is Japanese by default with English and 简体中文 selectable.
