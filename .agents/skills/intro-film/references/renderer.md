# Renderer notes

Constraints of the Python renderer in `scripts/film/` (`core.py`, `scenes.py`, `make.py`). Each one records a defect seen on the first build, so keep them when changing the renderer.

- Master is `DW x DH` design units (1040 by 720) times `S` (2). `CAP` is capture pixels per design unit for windows (2), `HUDCAP` for HUD captures (8). `put`, `shadow` and `window` take design units; sprites are built in master pixels.
- Time is a warp `KNOTS` from output time to script time; equal script values are holds. Anything driven by real captures after a hold must use output time (`TN`) so it keeps playing through the hold (command, meeting end, chat, History copy).
- Placement is sub-pixel (`place`: premultiplied affine, bilinear for sprites, bicubic only for opaque windows, 3x supersampled rounded masks). Integer rounding made windows jitter.
- Dither the stage before 8-bit quantization (concentric banding rings otherwise) and fade the post-dither out before the hand-over so the last frame stays clean.
- Captures are about 15 to 20 fps: `Seq` blends neighbouring frames only where pixels change a little (glows, progress). Unconditional blending ghosted text. Keep it gated.
- Anti-alias chip and pill sprites (`rounded_sprite`); aliased 1 px outlines turned into white fringes when moved by fractions of a pixel.
- Chroma downscale with `flags=area`. Lanczos chroma rang around saturated shapes like the green dot.
- The offscreen welcome capture is one device pixel taller than the live window, which moves the centred page half a point. `WELCOME` is cropped from row 1. Measure alignment against a live screenshot (phase correlation of the wordmark, tagline and button); it was 1.0 px before the fix and 0.0 to 0.1 px after.
- Keep camera moves to few scale changes, slow, with smootherstep; never shrink a window to a point. The button sweep follows the real button box measured per language (the Japanese, English and Chinese buttons are 140, 157 and 110 px wide).
