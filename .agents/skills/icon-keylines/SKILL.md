---
name: icon-keylines
description: Size and center Dito's 24 pt outline icons with one shared rule (keylines). Run it whenever an icon is added to assets/icons/lucide or `design:components/icons`, an icon's shape is edited or re-imported, or an icon looks too big, small or off-center in a circle. Measures, syncs FigCraft masters and instances, bakes the SVGs.
---

# Icon keylines

One rule sizes every icon, so new and edited icons reach the same optical size without hand tuning. The rule and its numbers are in `design/README.md` under "Keylines"; these scripts apply it. Run them, do not read them into context.

Prerequisites: `uv` (each script declares its dependencies inline), `figcraft-cli`, and for live steps a FigCraft window started with `figcraft-live` `ensure` (see that skill). Paths below assume `S=.agents/skills/icon-keylines/scripts` and `I=crates/dito-app/assets/icons/lucide`.

Out of scope: native app artwork, `icons/illustrations`, `icons/hud/auto.svg`.

## Workflow

The design leads, so the FigCraft masters change before any SVG does.

1. **Plan.** Measure and read the table (scale `s`, shift `tx ty`, reason `mark`/`area`/`cap`/`+floor`). Nothing is written.

   ```sh
   uv run --script $S/keylines.py plan $I --no-shift check --out /tmp/plan.json
   ```

   `--no-shift check` keeps the approved 1 pt drop of `check`, which the HUD outcome contract depends on. The last line counts the icons that change. A normalized set can still report one residual 0.5 pt shift (`download`, whose first shift was capped at 1 pt); bake it and plan again until the count stops changing.

2. **Sync the design.** Dry run first; it counts the commands and edits nothing. Then apply it to the open document.

   ```sh
   uv run --script $S/figcraft_sync.py sync design/dito.figcraft /tmp/plan.json
   uv run --script $S/figcraft_sync.py sync design/dito.figcraft /tmp/plan.json --apply --project "$PWD"
   mise run check:design
   ```

   `--apply` scales the masters, resets every instance to follow them, saves, repairs what the reset disturbs (instance sizes, scaled HUD shadows and edge strokes, bindings, rectangle corners, drifted instances) and saves again. A master with no SVG (for example `lucide/circle`) is not in the plan; add its `s`, `tx`, `ty` to the plan JSON first. If `check:design` fails, read the failing master or instance and rerun `sync` only after the cause is fixed, because a second run scales the masters again.

3. **Bake the SVGs** from the same plan so code follows the design.

   ```sh
   uv run --script $S/keylines.py bake $I --plan /tmp/plan.json          # report only
   uv run --script $S/keylines.py bake $I --plan /tmp/plan.json --write
   ```

   `bake` rewrites path coordinates and leaves the stroke at 2 pt. An SVG that is not in the design-tool export form (`<path transform="matrix(...)">`, commands M L C Z) is wrapped with a compensated `stroke-width` and reported as `wrapped`; re-export it from its master when you can.

4. **Review.** Look at every icon at native size in a circle, then run the project checks.

   ```sh
   uv run --script $S/keylines.py sheet $I /tmp/sheet.png
   ./scripts/check.sh fast && mise run lint:rust
   ```

## Adding a new icon

1. Put the SVG (Lucide source, `currentColor`, 24 pt canvas) in `$I` and run step 1. Read only its row.
2. If it changes, run step 3 on the directory: the other icons of a normalized set have `s` = 1 and stay as they are.
3. Create the master from the baked SVG. `--parent` is the id of the frame that holds the other masters (read it from any `lucide/*` component's parent). Supported shapes: path, line, polyline, circle.

   ```sh
   uv run --script $S/figcraft_sync.py master <name> $I/<name>.svg /tmp/master.json --parent <frame id> --x <x> --y <y>
   uv run --script $S/figcraft_sync.py apply /tmp/master.json --project "$PWD"
   ```

   The batch builds a clipping 24 pt component with no fill, one vector per shape, a 2 pt round stroke bound to the shared stroke variable and scale constraints, the same as the existing masters. Add the `Ic` mapping in `crates/dito-app/src/assets.rs`, save the design, and run `mise run check:design`, which rejects a master with an ink side above 20 pt or a center more than 1 pt off the canvas.

## Gotchas

- FigCraft's `transform.scale` scales strokes and does not reach existing instances. `sync` restores the 2 pt stroke and resets each instance; do not hand-edit masters without it, or every screen keeps the old geometry while `check:design` fails.
- Resetting an instance also resets its stroke weight and can reset scaled HUD effects. The repair step restores them, so do not stop after `masters` and `instances`.
- `icons/hud/copy-front.svg` and `copy-back.svg` mirror the `.motion/copy-front` and `copy-back` masters, not `lucide/copy`. If the copy construction changes, regenerate both from those masters and recheck the Copied outcome in `design:hud/processing`.
- Never wrap an SVG in a scaling `<g>` to fix size, and never add per-icon scales or offsets at call sites. Both split the design from the code.
- `file.save` through figcraft-live needs an absolute path; `sync` and the commands above already pass one.
