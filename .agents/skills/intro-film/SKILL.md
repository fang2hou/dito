---
name: intro-film
description: Make, rebuild or integrate Dito's first-launch intro film (assets/intro, onboarding step before the welcome page, README videos). Use when the film needs new captures, a new language, a retime, a new soundtrack, a higher resolution, a README upload, or when the in-app player or its hand-over to the welcome page changes. Covers capture, the Python renderer, audio through OpenRouter, encodes, the app integration and the pitfalls found on the first build.
---

# Intro film

A one-time film plays in the wizard window before the welcome page, ends on a frame identical to that page, and hands over without a visible change. The design is `design:onboarding` (board 22.10 in `design/dito.figcraft`); the storyboard is `design/intro-film.figcraft`, whose board 20.00.4 lists the copy per language (ja, en, zh, ko), so add a column there for a new language. Scripts live in `scripts/` next to this file; they are reference code for one machine and take their paths from environment variables.

## Principles

| Rule | Why |
| --- | --- |
| Every Dito screen, HUD capsule, editor text and the welcome page is the app's own render, never redrawn | Hand-drawn capsules and dot meshes were rejected as "not real" |
| Only the stage, window masks, typed intro lines, word chips and transitions are drawn in code | Those have no real counterpart |
| Captures differ per language: detect each language's event times, never reuse the Japanese ones | The English and Chinese meeting captures ended on the permission page, the copy check and chat timing differed |
| Never freeze a frame where a real animation is mid-flight; hold after it settles. A hold is plain stillness, with no forced zoom | A frozen half-drawn check read as a bug; a push-in during holds read as unnatural |
| Plays once, can be skipped, never makes the user wait: at most 4 s per shot in the first plan, the shipped film is about 60 s | The owner asked for calmer pacing after the first cut |
| Languages: ja, en, zh and ko have their own film; zh-Hant uses the zh film; mn and fr use the English one (`film_name` in `intro.rs`) | Scope from the owner |

## Pipeline

1. **Capture** with the screenshot mode of a patched build. `scripts/capture-patch/` holds `intro_film.rs` (the interaction scene: Wordbook add, model download and tile, History open and copy) and patches for `HUD_SCALE`, a HUD-only and editor export in `readme_film.rs`, and the registration in `mod.rs`, `runner.rs`, `pages.rs`. Apply them to a scratch copy of the repo, never to the repo. Build with `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`; clone `target/local` with `cp -cRp` to avoid a cold build.
   - Window scale follows the display. On a 4K display set to 1920 by 1080 the offscreen windows render at 2x and a HUD at `HUD_SCALE` 4 renders at 8x, with no code change. On a 1x display everything is half as sharp, so check the image size first.
   - Run `scripts/film/capture.sh <lang>` for one language at a time (parallel runs lower the capture frame rate to about 15 fps) with fresh data dirs, because state persists in the data dir.
   - Each language produces `hud_<l>` (dictation and command HUD plus editor frames), `intro_<l>` (Wordbook, Models, History and `welcome-dark.png`) and `tools_<l>` (meeting and chat, light theme).
2. **Detect events** per language from the frames: spikes of frame difference give the meeting stop and end, the chat send and settle, the copy click (about 3.35 s) and the HUD states. Put them in `EVT` in `scenes.py`. The last two meeting frames are the permission setup page; end before them.
3. **Voice and music** (see Audio): `tts.py`, `process_voice.py` and `word_times.py` in `scripts/film/`. Word timestamps of the spoken line drive the word chips.
4. **Render** with `scripts/film/make.py` (`FILM_LANG=ja python3 make.py out.mp4`), about 12 minutes per language and three languages in parallel. It writes the HEVC master, an H.264 copy and the app encode in one pass from raw frames.
5. **Mix** with `audio.py` (SFX synthesized on the cue sheet, voice, ducked music, -14 LUFS) and mux.
6. **Install** the app encodes as `assets/intro/dito-intro-<lang>.mp4`, run the app (below) and compare the last frame with the live welcome page.

A new language needs: its `Words` entry in `readme_film.rs` (Korean already exists), its code in `intro_film.rs` (`capture-patch` has the `ko` line), a `TEXT` entry (typed intro lines, chips, clean line), an `EVT` entry detected from its captures, voice and word times, a font in `core.py` (Korean uses the system Apple SD Gothic Neo), then `film_name`, the design note on 22.10 and a test. Compare the command timeline: Korean takes about 1.5 s longer before the text is replaced, handled by `CS` in `scenes.py`. Lyria has no usable seed, so a language added later gets a new music take; say so.

## Renderer notes

- Master is `DW x DH` design units (1040 by 720) times `S` (2). `CAP` is capture pixels per design unit for windows (2), `HUDCAP` for HUD captures (8). `put`, `shadow` and `window` take design units; sprites are built in master pixels.
- Time is a warp `KNOTS` from output time to script time; equal script values are holds. Anything driven by real captures after a hold must use output time (`TN`) so it keeps playing through the hold (command, meeting end, chat, History copy).
- Placement is sub-pixel (`place`: premultiplied affine, bilinear for sprites, bicubic only for opaque windows, 3x supersampled rounded masks). Integer rounding made windows jitter.
- Dither the stage before 8-bit quantization (concentric banding rings otherwise) and fade the post-dither out before the hand-over so the last frame stays clean.
- Captures are about 15 to 20 fps: `Seq` blends neighbouring frames only where pixels change a little (glows, progress). Unconditional blending ghosted text. Keep it gated.
- Anti-alias chip and pill sprites (`rounded_sprite`); aliased 1 px outlines turned into white fringes when moved by fractions of a pixel.
- Chroma downscale with `flags=area`. Lanczos chroma rang around saturated shapes like the green dot.
- The offscreen welcome capture is one device pixel taller than the live window, which moves the centred page half a point. `WELCOME` is cropped from row 1. Measure alignment against a live screenshot (phase correlation of the wordmark, tagline and button); it was 1.0 px before the fix and 0.0 to 0.1 px after.
- Keep camera moves to few scale changes, slow, with smootherstep; never shrink a window to a point. The button sweep follows the real button box measured per language (the Japanese, English and Chinese buttons are 140, 157 and 110 px wide).

## Audio

- Keys: the owner keeps a temporary OpenRouter key in a file. Read it into an environment variable (`OR_KEY`), never print it or write it into a file, and suggest revoking it afterwards. Check `GET /api/v1/key` for the limit and usage.
- Voice: `POST /api/v1/audio/speech` with `elevenlabs/eleven-v4-turbo`, voice `sarah`, `response_format` `mp3`. Gemini TTS drops fillers and only returns pcm. Trim silence, adjust tempo (ja 1.12, zh 1.1), loudnorm. Verify by transcribing with `openai/whisper-large-v3-turbo`. Word times come from `openai/whisper-large-v3` with `response_format` `verbose_json` and `timestamp_granularities` `["word"]`; it drops fillers, so interpolate those.
- Music: `google/lyria-3-pro-preview` through chat completions with `modalities` `["text","audio"]` and streaming, about $0.08 for a 63 s mp3. The whole build cost about $0.11.
- SFX: synthesize with numpy, one tonal family in D major pentatonic. No noise whooshes: the air-sucked whoosh was rejected. A scene change is one quiet muted marimba note plus a felt tap. Add a short reverb bus and a faint room tone so cuts never reach digital silence.
- The music fades out before the green dot, which gets a pitch-glide pop and a bell; the button sweep gets one soft high glass note.

## README images

`scripts/readme-assets.py` builds `assets/readme/*.webp` from the same screenshot scenes (`mise run readme:assets`). The first version was soft because it captured at 1x and resized to 800 px. Now: run it on the Retina display, frames are scaled to 1440 px for the 720 pt README slot, each frame gets continuous (Apple style) rounded corners from a Figma-style squircle path drawn at 4x, and the WebP is lossless when it fits under 9 MB (all of them do, under 1 MB). The HUD placement in `readme_film.rs` scales with the captured layer; with fixed 1x offsets the capsule fell outside the frame on a 2x display.

## Encodes

| Output | Use | Settings |
| --- | --- | --- |
| Master | viewing | HEVC 10-bit, `hvc1`, BT.709 limited, CRF 15 |
| H.264 | compatibility | 8-bit, CRF 14, `-tune grain` |
| App | `assets/intro/` | HEVC 8-bit, yuv420p, full range, BT.601 matrix (`smpte170m`), CRF 16, made from raw frames |
| GitHub | README upload | H.264 two-pass at about 9.3 MB total, same resolution and frame rate, SSIM about 0.9975 |

The app encode must be BT.601 full range because gpui's surface shader decodes two-plane 8-bit YCbCr with the 601 matrix and does no colour management; the decoded frame then matches the pages drawn by the same renderer (PSNR 51 dB against the real welcome capture before the alignment fix). GitHub inlines a video only from a `user-attachments` URL on its own line; the owner uploads (about 10 MB limit on a free account), then the URL goes into `README.md`, `README.ja.md`, `README.zh-CN.md` and `README.ko.md` under the intro paragraph, one per language film (Traditional Chinese has no README of its own).

## App integration

- `dito-platform` `system/intro_video.rs`: `IntroVideo` (AVPlayer plus `AVPlayerItemVideoOutput` asking for `420f` with IOSurface and Metal compatibility). It only decodes and plays the sound.
- `dito-app` `onboarding/intro.rs`: paints each frame with gpui `surface`, the skip button above it, and removes the layer when the film ends. The welcome page is already under it, so there is nothing to swap. A native overlay view was rejected because the skip button could not draw above it.
- Dark welcome: when the film played and the appearance is light, the welcome page keeps the dark palette until the first step slides in.
- Controls: hold Ctrl for 3x, Esc or the button to skip (volume fades, the button leaves 8 s before the end). `DITO_INTRO=always` or `never`, `DITO_INTRO_DIR` for the films, the `intro-seen` marker in the data dir (mock runs keep none). It never plays headless or with reduced motion.
- Bundling: `scripts/bundle.sh` copies `assets/intro/*.mp4` into `Contents/Resources/intro`; `scripts/dev-runner.sh` passes `DITO_INTRO_DIR` because the dev wrapper has no films. `prek.toml` allows files under `assets/intro/` up to 24 MB.
- Test with a real run, since mock runs are already onboarded and open no wizard: `DITO_INTRO=always cargo run -p dito-app -- --data-dir /tmp/dito-fresh`. Take a burst of `screencapture -x` shots around the film end and compare the last film frame with the first welcome frame.

## Pitfalls

- gpui: `Window::on_key_event` panics outside paint; use element handlers (`track_focus` plus `on_key_down`). A glob import of the UI framework in a test module brings its own `test` attribute and recurses; import names explicitly.
- Shell: never run a bare `git stash`. `pgrep -f` matches your own command line. `wc -l` pads with spaces on macOS, so compare numerically. Background `until` loops need a timeout.
- FigCraft: `.figcraft` is a zip, so a merge conflict is binary. Take develop's file, then copy the board across tabs (`edit.copy`, switch tab, `edit.paste`); pasted instances copy their components, so `instance.swap` them back to the shared ones. A replaced image stays in the archive: run `scripts/prune_figcraft_images.py`, then reload the tab so it does not save the image back. Placeholder stills belong in the file as small JPEGs. To tidy a file: run the prune script in dry run (it must report 0 unreferenced), check there are no unreachable or stray nodes, save with an absolute path (`file.save` rejects repo-relative ones), and `stop` the instance you started so its extra Untitled tab does not linger.
- The pre-commit hook rejects files over 500 KB; the intro films need the `prek.toml` exception.
- Check the size of the first captures: with a second 1x display as the main display the Korean captures came out at 1x (1040 by 720 welcome) and had to be redone. Only the 4K display may be connected.

## Cleanup after a build

Delete the scratch capture build and captures, the render output folders, voice and music intermediates, the temporary key file once revoked, and temp files under `/tmp`. Keep the finished films, the repo assets and this skill.
