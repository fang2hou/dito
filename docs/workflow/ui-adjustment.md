# Workflow: Adjust the UI

Use this for any visual or motion change: a detail, a new state, a new animation. The design file `design/dito.figcraft` is the source of truth; the code follows it.

## 1. Place the change

Decide whether the element is shared or local before touching anything.

| The element is… | Edit it on |
| --- | --- |
| A shared component or shared motion (switch, segmented control, dropdown, slider, tooltip, tokens) | `11 Type, Space & Components` or `12 Motion` |
| Part of one screen (a HUD state, a settings page, an onboarding step) | That screen's section on pages `20`–`22` |

If an existing element sits in the wrong place, move it first, then change it. Conventions for sections, captions and note cards are in [design/README.md](../../design/README.md).

## 2. Update the design

1. Open the design through the `figcraft-live` MCP (`ensure` with `design/dito.figcraft`).
2. Inspect the current style of the element and of its neighbors, then make the change: adjust details, add states, or add an animation as a storyboard with timings and a token from `12.01` (or `21.05` for the HUD).
3. After any content or layout change, inspect all affected ancestor backgrounds, including shared-component boards and outer sections. A section does not grow automatically with its auto-layout board. Resize it in the same edit to maintain the 40 pt inset on all four sides; a single-board section is the board size plus 80 pt in each dimension.
4. Render the entire affected section, including the background's bottom edge, and inspect its solved bounds. Checking only a selected screen or component is insufficient. Cards, notes and examples must remain inside their board and section; an unchanged background with overflowing content means the task is not finished.
5. Save with `file.save`. Save after every change; the user rolls back if needed.
6. If the change placed a new image (for example a new app icon render), remove the images it replaced. FigCraft keeps every image ever placed inside the file, even after no layer uses it, and has no command to drop them, so old renders quickly push the file toward GitHub's 100 MB limit. Close the file in FigCraft, remove the `images/` entries and `imageIndex` items that no layer's image fill refers to, reopen it, check the render, and save. Delete old version snapshots (`version.delete`) at the same time if the file is still large.

## 3. Get approval

Point the user at the section that changed and wait. Code work starts only after the user has reviewed it in FigCraft and approved it.

## 4. Implement

1. Change the code so it matches the approved section, and name the section in the reply.
2. Run `./scripts/check.sh fast` and `mise run lint:rust`.
3. For tooltips and hover overlays, verify entrance, exit, rapid leave and re-entry, keyboard dismissal, and Reduce Motion. Preserve the settled anchor and keep the visual layer mounted through its exit; never replace animated opacity with an immediate zero when hover ends.
4. Don't write tests for look and feel. Animation feel, timing and color are checked by the user in the running app.
5. Before asking for that check, start the app (`mise run dev:mock`, with `-- --data-dir <empty dir>` appended to land on onboarding) and say exactly what to do and where to look.
