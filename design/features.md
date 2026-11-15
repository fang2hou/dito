# Dito: Goals and Features

What the app is for and what it does, from the user's side. How it looks and moves is in `dito.figcraft` (conventions in [README.md](README.md)); how it is built is in [docs](../docs/README.md).

## Goals

- **Speak instead of type, anywhere.** One shortcut, then talk; polished text appears where the cursor is, in any app.
- **Never lose what was said.** If polishing fails, the raw text is inserted. If there is nowhere to type, the text goes to the clipboard. Every result stays in History.
- **Stay out of the way.** The HUD never takes focus, never covers menus, and disappears when done. Starting Dito opens no window. Settings change in one place each.
- **Local first, cloud when wanted.** Speech recognition and polishing each run on the Mac or through a provider, chosen separately.
- **Japanese first.** The UI is designed in Japanese; English and Simplified Chinese are available. The speech language is a separate setting.
- **Calm and precise.** Short, purposeful motion; no flashing; Reduce Motion respected everywhere.

## Dictation

- Press the shortcut (right ⌘ alone by default), speak, and the speech is transcribed, optionally polished, and typed at the cursor.
- **Toggle mode** (default): press to start, press again to finish; while recording, a press of either shortcut (dictation or command) finishes the run, whichever kind it is. **Hold mode**: speak while holding, release to finish.
- Esc or × cancels while recording, for a dictation and for a command; ✓ finishes; Stop cancels transcription or polishing.
- A lone-modifier shortcut followed quickly (within 0.4 s) by another key counts as typing, not as a dictation. It only drops the recording that press just started, never one that was already running.
- Recordings stop at 5 minutes; a countdown appears in the last minute.
- Insertion uses key input (default) or paste, with automatic fallback, and restores the clipboard afterwards.
- Other audio is muted while the microphone is live (on by default).
- Silent or very short clips are discarded quietly.
- The polishing style follows the app in front: the built-in Slack, Teams and Herdr styles when those apps are installed, custom app styles, or Other.

## Command

- Select text in any app, press the command shortcut (Shift plus the dictation shortcut by default, so ⇧ right ⌘ at first; it follows the dictation shortcut until you set your own in General) and say what to do with it: polish, rewrite, translate, summarize, reply to it or ask something about it. A polish, rewrite or translation replaces the selection; a reply is the response to a message you received, shown for confirmation with Copy only, never written over the message or inserted. Text that cannot be edited (a preview, a web page, a read-only view) is never written over: the confirm window opens with Copy only, as for a reply, and only an editable selection whose write fails falls back to the gold Copied outcome. Apps that expose no text to Accessibility, such as Zed, are read by copying the selection (also in a read-only preview, where the Edit menu's Copy item stays disabled); the clipboard is put back as it was, and the result is pasted over the selection when the app's Cut item shows it can be edited.
- The two keys of the default command shortcut can go down in either order. When the dictation modifier lands first and Shift follows within 0.4 s, the recording that just started becomes a command in place (the selection is read then and the capsule turns blue) instead of being cancelled and started again; Shift added later is ignored. Recording starts at the first key either way, but the capsule waits up to 150 ms for Shift: with Shift it pops in blue and never gold, without it pops in gold 150 ms later than a plain dictation, and a typing chord such as Cmd+C inside the wait shows no capsule at all. Shift after the wait fades the visible gold capsule to blue over 160 ms (the reverse handover fades back the same way). The gap between the two keys is logged as `chord_gap_ms` for tuning the wait. The selection is read again when the recording ends, so you can change it while you speak; if that second reading fails, the first one stands. In toggle mode press once, speak, and press again to finish; Esc cancels.
- The instruction can be typed instead: hovering the capsule shows a keyboard button that opens a field. A polish button next to it polishes the selection at once.
- The capsule turns blue for a command and shows only the state: transcribing, thinking, working, then a blue check.
- A small window opens above the capsule only when needed: to choose between tools when the instruction is unclear, to enter text when nothing is selected, to confirm a result before it replaces the selection (always for a summary, for everything with the setting on), and to show the answer to a question. A reply always waits for confirmation. Something Dito cannot do, such as "send this to Sato", is not a command: nothing runs, and a gray capsule says the instruction was not recognized, then hides after 3 s. A question asked with nothing selected is answered on its own, without the text of the field around the cursor.
- A command needs a local text model, which also decides what the instruction asks for.
- One step per instruction for now.

## Transcription

- Lives in the Tools window (ツールウィンドウ), outside Settings, opened from the menu bar (ツールを開く), the Dock menu or its global shortcut (Control+Option+T by default, set in General under システム). The 書き起こし page is the first tool under ツール.
- Started and stopped from the bottom Dock Panel; nothing is recorded until the user starts a session. At rest it is only the 新しい会議 button; sources are chosen while recording, not before. Pressing it folds the button into the 停止 circle at the panel's right end while the panel widens and grows into one live lane per source above a bar with the timer, the title, the two source toggles and a 一時停止 circle that freezes the clock while the session is paused; the controls grow out of the center and keep fixed 8 pt gaps. A start that takes longer than a moment shows 準備中 first. Stop asks once: the first press widens the circle into a 停止する？ pill that flashes once and keeps a soft halo, the second ends the meeting, and a pause, Esc or four seconds without a press folds it back into the circle. Browsing the list or an older transcript during a session tucks it back into that bar. Stopping keeps the transcript open and folds the panel into a status capsule that only reads 整形中 while the last words are polished, then returns it to the 新しい会議 button. The button is available beneath both the list and finished transcripts. Starting a session opens its new transcript.
- Before the first session, a readiness check lists what is missing: the マイク and システムオーディオ録音 permissions and the voice activity model, each with one fix.
- Records one or both sources the user picks: デスクトップ音声 (what the Mac plays, through a system audio tap on macOS 14.6 or later) and マイク (the user's own voice, such as a long monologue). With both, meant for headphones, the two are kept apart as 自分 and 相手. Either source can be turned off or back on during a session (at least one stays on): an off source is closed, so the macOS microphone indicator goes out, and its lane folds away.
- Speech is cut into utterances by Silero VAD and recognized as each one ends with the speech engine chosen for dictation. While an utterance goes on, it is recognized again about every second when the engine is free; the final recognition of a finished utterance always goes first and cancels a partial one in progress.
- The paragraph still being heard stays in its speaker's lane of the Dock Panel for the whole session: its settled words in normal text, then the unsettled words with a sweeping gradient and a trailing ellipsis (trailing punctuation removed). Unsettled words stay until their final text replaces them. Only closed paragraphs are rows, first marked 整形中 and then editable; a reader at the end of the entry stays at the end as rows arrive.
- Utterances are grouped into paragraphs at a 2 second pause, a change of speaker, the first sentence end past 80 characters (about three sentences), or 280 characters without one. A closed paragraph goes to polishing at once.
- Each paragraph is polished with the two paragraphs before it as read-only context, plus the wordbook when it applies to polishing. A paragraph keeps the recognized words if polishing fails or is off.
- Each session is one entry, like a memo: a title, the start time and length, and its paragraphs with time and speaker. Finished paragraphs are edited in place; an entry can be copied as text or deleted.
- The voice activity model (Silero VAD, about 0.6 MB) is downloaded from the 発話検出 tab of the Models page, like every other model.

## HUD

- A small capsule above the Dock, or following the pointer. It never takes focus and stays above app windows. Only a command window with a text field takes the keyboard, and gives it back when it closes.
- States: listening (with a live waveform), transcribing, polishing, inserted, copied, and errors (authentication, no speech, microphone, network, other).
- Errors explain the reason and offer Retry or Open Settings; they close after 3 seconds, and hovering keeps them open.
- Hovering shows the speech language selector, button tooltips and the Stop button.
- Its appearance (system, light or dark; dark by default) is set separately from the settings window.

## Settings

| Page | What it does |
| --- | --- |
| 一般 (General) | Shortcut, record mode, insert method, command shortcut (with a button back to the default; never the same as the dictation shortcut), music mute, microphone (automatic or fixed), speech language, display language, appearance, HUD appearance and position, launch at login, the Tools window shortcut, restart onboarding |
| モデル (Models) | Three tabs: 音声認識, テキスト and 発話検出 (the Silero VAD used by transcription, local only and active once downloaded). Local models (download, pause, resume, verify, delete, switch with 使用する, move the folder), with request parameters per local text model; cloud configurations for speech and for polishing, shown as a scrolling row of tiles that can be added from a template (FunASR, Qwen ASR, OpenRouter for speech; Azure OpenAI, Amazon Bedrock, OpenAI-compatible for polishing), duplicated, renamed, reordered by dragging and deleted; cloud speech edits the full request in the inline JSON editor with valid-draft autosave, native undo and redo, and a single documentation help dialog with four scrollable reference cards; cloud polishing has request parameters (temperature, top-p, max tokens, reasoning effort, extra JSON); connection tests |
| 単語帳 (Wordbook) | Words passed to speech recognition as hotwords, optionally also to polishing; import, export and confirmed bulk deletion; a banner when an action fails; finding words in pasted text or in a Markdown, plain text, Word or PowerPoint file with the text model, or in the reply of another AI that was given a copied prompt and files Dito cannot read (PDF, images), then choosing which to add |
| プロンプト (Prompts) | Shared rules and app writing styles per language, installed-app icons, add or remove an app style, copy settings to another app, optional style examples and a safe preview. `design:settings/prompts`: taller editors, grouped undo and redo, draft autosave, scoped reset confirmation and plain-text before and after panes |
| 履歴 (History) | Search and filter by period, text or app; entries from configured apps carry the app name and the rest read as Other; each entry shows the polished text and the original; transcribe again, polish again, copy, play audio, delete |
| 統計 (Stats) | Time saved first, then characters, recordings and speaking speed; characters per day as a week of tiles, a month calendar or a heatmap since first use, following the period, top apps and languages |
| ステータス (Status) | Whether dictation works right now: a verdict, then shortcut, microphone, speech recognition, polishing and insertion in order, each with what it uses or how long it takes, and the reason and one fix when it fails; the システムオーディオ録音 permission for transcription is listed too but never blocks dictation |

The sidebar groups pages under 設定 (Settings) and 利用状況 (Activity). It shows a health dot (green when ready, red with the reason when a local model is stopped) and collapses to a rail in narrow windows.

## Models

- Local speech recognition: Qwen3-ASR 1.7B (default, most accurate) and 0.6B, both with hotwords; SenseVoice Small (fastest). Optional downloads to compare: Confucius4-R2T2, Fun-ASR-MLT-Nano-2512, whisper-large-v3 and whisper-large-v3-turbo (all with hotwords), and Haas-ja v1.0.
- Local polishing: Qwen3.5 Instruct 2B, 4B and 9B.
- Each local model lists from small to large and carries labels for its parameter count, quantization, language (speech models) and download size. Hovering a label explains it in everyday words (`design:settings/local-models/labels`).
- Nothing downloads until the user asks; downloads resume and are verified. Models in use start at launch.

## Menu Bar

A waveform icon showing the engine status and today's character count, with shortcuts to the Tools window, History, Wordbook, Status, Settings and Quit. It never changes settings.

## Dock

- **Dock icon (General, on by default).** Off, Dito runs from the menu bar alone and has no Dock tile. While a window is open the tile comes back, so the window can be found with the Dock and Cmd-Tab, and it leaves again when the last window closes.
- **Dock menu.** Right-clicking the tile lists ツールを開く, then Settings and the History, Wordbook and Status rows of the menu bar menu, with no status line. Like the menu bar, it only opens windows and pages.
- **Reopen.** Clicking the tile, or opening Dito again from Finder or Spotlight, while it runs with no window opens Settings on General.

## Onboarding

Shown on first launch and restartable from General, in seven steps: welcome, permissions (Microphone and Accessibility, plus System Audio Recording for transcription, which never blocks continuing), how to use (with a looping demo), recording shortcut, engine choice (local or provider), preparing the models or the connection, and trying a dictation. Steps slide forward and back; with Reduce Motion they cross-fade.

## Behavior Details

Design decisions that aren't visible in a single screen. Timings and sizes themselves live in `dito.figcraft`.

- **Recording limit.** Recordings stop at 5 minutes and are processed, never discarded. The countdown appears at 4:00, in its own reserved space so the HUD never moves, and turns amber for the last 10 seconds.
- **Music mute.** Other audio is muted only while the microphone is live, and only if something is playing and the output isn't already muted. When recording stops, the previous state is restored, not simply unmuted.
- **Automatic microphone.** Automatic picking opens only wired devices (built-in, USB, Thunderbolt and similar). Opening a Bluetooth headset drops it to its low-quality call profile, and opening an iPhone wakes it as a Continuity microphone, so those are chosen by hand.
- **HUD size.** The capsule is 105 × 33 pt (72 pt wide in hold mode while listening), drawn at 1× in whole points, with opaque surfaces and one soft shadow.
- **HUD screen.** The HUD opens on the screen under the pointer and changes screens with it, in both position modes. Centered moves when the pointer enters another screen; while a mouse button is held, it waits for the release.
- **Outcome timing.** The HUD hides when its outcome animation ends: about 1.1 s after Inserted and 1.9 s after Copied. Errors stay 3 s, and hovering keeps them open.

- **Silent launch.** Starting Dito, at login or by hand, means the user wants to dictate, not to configure. No window opens, the app never takes focus, and the menu bar icon is the sign that it runs. Three things may open a window by themselves: Onboarding on first run, Settings when a permission is missing, because dictation cannot work without it (General when Accessibility is missing, where the banner explains it; Status when only the Microphone is denied), and the startup failure window when the settings or data cannot be read, which shows the file and the reason and can restore default settings. Otherwise Settings open on request: menu bar, Dock menu, Dock click, or opening the app again while it runs.

## Known Gaps

- The display language follows the system by default; the goal is Japanese by default with English and 简体中文 selectable.
