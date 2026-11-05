# Dito design file conventions (FigCraft)

Writing rules (no `·` or `|` separators, natural phrasing) and the design-first workflow are in the repository's `AGENTS.md`.

Applies to every page, section, board, layer and annotation in `design/dito.figcraft`. The page `01 Conventions` inside the file is the short version; if the two disagree, this file wins.

## 1. Principles

| Rule | Scope |
| --- | --- |
| Documentation is English: page, section, board and layer names, notes, note cards, annotations | Whole file |
| Screen copy is Japanese and matches the `ja` values in `crates/dito-core/src/i18n/messages.toml` exactly | Every screen frame |
| English and 简体中文 screens appear only on `14 Localization` | Localized screens |
| The design leads and the code follows. Only the chosen design is kept; rejected options are deleted | All sections |
| Before changing anything, decide whether it is a shared component or belongs to one screen. Shared components are edited on `11 Type, Space & Components` or `12 Motion`; screen-specific parts are edited in that screen's section | All changes |
| Keep notes short: key values and rules, not the discussion that led to them | All notes |
| `12 Motion` holds shared component motion (switch, segmented, dropdown, slider, tooltip). Motion that belongs to one screen (HUD, History, Onboarding) stays on that screen | All pages |

## 2. Structure

```
Page     NN Name          e.g. 21 HUD & Menu Bar
Section  NN.MM Name       one board per section, e.g. 21.02 Recording
Item     NN.MM.K Name     one row group inside the board, e.g. 22.02.1 Microphone granted
```

| Range | Group | Pages |
| --- | --- | --- |
| 00–09 | Guide | `00 Cover & Index`, `01 Conventions` |
| 10–19 | `- Foundations` | `10 Color`, `11 Type, Space & Components`, `12 Motion`, `13 Dot Motif`, `14 Localization`, `15 Build Constraints` |
| 20–29 | `- Screens` | `20 Settings`, `21 HUD & Menu Bar`, `22 Onboarding` |
| 30–39 | `- Brand` | `30 App Icon` |

New pages and sections take the next free number; existing numbers are not compacted.

### Section map

| Page | Sections |
| --- | --- |
| 12 Motion | 12.00 Overview, 12.01 Motion Tokens, 12.02 Controls, 12.03 Slider, 12.04 Dropdown |
| 14 Localization | 14.01 Languages, 14.02 Localized Screens |
| 20 Settings | 20.00 Overview, 20.01 General, 20.02 Dictionary, 20.03 Prompts, 20.04 Local Models, 20.05 Cloud Models, 20.06 ASR Connection JSON, 20.07 History, 20.08 Stats, 20.09 Status, 20.10 Sidebar |
| 21 HUD & Menu Bar | 21.00 Overview, 21.01 States, 21.02 Recording, 21.03 Errors, 21.04 Menu Bar, 21.05 HUD Motion Tokens, 21.06 HUD Processing, 21.07 Width & Pop-ups, 21.08 Follow Pointer |
| 22 Onboarding | 22.00 Overview, 22.01 Welcome, 22.02 Permissions, 22.03 How to Use, 22.04 Shortcut, 22.05 Engine, 22.06 Cloud, 22.07 Try It, 22.08 Page Slide, 22.09 Usage Demo |

Settings sections follow the app's navigation order. The prompt editor matches gpui-kit's Textarea: 13 px text, 20 px lines, no letter spacing. Older numbers (`03A`, `10c`, `20.0x` from the short-lived Japanese layout) are retired; code comments cite the new numbers or token names.

## 3. Naming

| Object | Format | Example |
| --- | --- | --- |
| Page | `NN Name` | `22 Onboarding` |
| Section / board | `NN.MM Name` | `21.02 Recording` |
| Item title | `NN.MM.K Name` | `20.01.3 HUD height slider` |
| Screen frame | `Screen (theme)`, or `Screen (theme, language)` when not Japanese | `General (dark)`, `General (light, English)` |
| Keyframe caption | `elapsed` or `elapsed: state` | `190 ms: about 108 pt` |
| Component layers | `kind-name` in English kebab-case | `btn-primary-save`, `nav-general`, `hud-capsule` |

Layer names derived from UI copy use the English string of the same i18n key. About 400 deep layers whose copy has no key still carry Japanese; rename them when touched.

## 4. Layout

- Screen pages and `12 Motion` start with `NN.00 Overview` at the far left (title, purpose, section list).
- Sections run left to right in number order, top-aligned, 240 apart.
- Board: white, radius 24, padding 64, auto layout. Top-left: number chip (Gold 05), title (32), one-line purpose (15, muted).
- Item: title (20), optional one-line note, then rows. Each frame sits in a cell with a small caption above it.
- Light on the left, dark on the right. When a row holds only one theme, a small gold "Light" or "Dark" label sits above the row and the captions drop the theme.
- Captions are 3 to 5 words. Longer explanation goes in the Notes card.
- Storyboards run left to right in time order.
- A note card (360 wide, 1–3 lines) closes the first row of an item on the right. It sits under an outlined gold "Notes" tag and stretches to the row's height, so its top and bottom line up with the frames beside it. The card is white with a 1 px Gray 02 border and radius 16. Never let a note float alone in a corner.
- Documentation styling comes from this file's existing boards and palette (outlined gold tags, white cards with a Gray 02 border, Gold headings). Don't bring in generic web patterns such as colored side stripes or filled badges.
- Nothing floats on the canvas.

## 5. Notes and annotations

| Where | What | Length |
| --- | --- | --- |
| Board purpose | What the section covers | One line |
| Item note | The rule a reader needs before looking | One line |
| Note card | Key values, token names, edge cases | 1–3 lines |
| Annotation on a layer | What the element does on click / hover, or how it moves | One sentence |

Annotation categories: **Interaction**, **Motion**, **Content**, **Measure**. Annotate controls whose behavior isn't visible in a still frame (buttons that open windows, sliders, pop-ups, anything animated).

## 6. Motion

- `12 Motion` covers shared components: tokens (12.01), controls (12.02), slider (12.03), dropdown (12.04). Values match the code.
- Motion that belongs to one screen stays with it: HUD tokens and storyboards on 21.05–21.08 (Copied sits beside the check on 21.06), History hint on 20.07.3, onboarding on 22.08–22.09.
- Screens cite a token name instead of restating shared timings.
- Every motion spec states: trigger, change, token (or duration + easing / spring), what happens if interrupted, and the Reduce Motion fallback (no stretch, overshoot, scale or travel; a short fade instead).

## 7. Tokens

| Collection | Modes | Content |
| --- | --- | --- |
| `Color` | Light / Dark | `Palette` semantic colors from `crates/dito-app/src/theme.rs` (`bg`, `side`, `card`, `text`, `muted`, `accent`, `danger`, …) |
| `Motion` | — | Shared: `motion/hover` 160, `tip` 150, `ring` 180, `sidebar` 260, `switch` 380, `glass` 460. HUD and onboarding: `pill` 150, `hud-pop` 420, `hud-widen` 300, `hud-label` 120, `onboarding-slide` 360 (ms) |
| `Size` | — | `radius/input` 12, `radius/card` 16, `radius/window` 20, `window/width` 1120, `sidebar/width` 232 |

Existing layers are not bound to variables yet; new components bind from the start.

## 8. Language in the product

- The app's first impression is Japanese: it is built for Japanese customers and users.
- Users can switch to English or 简体中文 (or follow the system) in General → 表示言語. Switching is instant across settings, HUD and menu bar.
- **Code gap:** `DisplayLanguage` currently defaults to `System` (`crates/dito-core/src/config.rs`), so a Mac set to English starts in English. The design calls for Japanese as the default.

Type: Japanese and mixed text use M PLUS 1p; Latin-only text uses Outfit; 简体中文 uses IBM Plex Sans SC; macOS fallback is Hiragino Kaku Gothic Pro.

## 9. Build constraints

The app is drawn in Rust with GPUI. Avoid, or check with engineering first:

- Background blur or frosted glass on in-app surfaces → opaque color plus one soft shadow.
- Inner shadows, blend modes, free-form masks.
- Gradient, outlined or path text.
- Rotating arbitrary elements, 3D (SVG icons and images can rotate).
- Custom colors or radii in native menu bar menus.
- Moving the title bar or traffic lights.
- Motion without numbers.

Engineering background: `docs/engineering/ARCHITECTURE.md` and the ADRs in `docs/engineering/adr/`.

## 10. Workflow

1. Decide where the change belongs: a shared component (`11`, `12`) or one screen's section. Edit it there and nowhere else.
2. Add the section at the end of its page with the next number; build it from an existing board.
3. Copy comes from `messages.toml`; settle new copy in design before adding a key.
4. Shared component motion goes on `12 Motion`; screen-specific motion stays in that screen's sections.
5. Add annotations to interactive or animated layers.
6. Render light, dark and localized screens with `figcraft-cli render` and check for overflow.
