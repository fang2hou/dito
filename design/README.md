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
| 11 Type, Space & Components | 11.01 Type, Space & Components, 11.02 Icons, 11.03 Controls |
| 12 Motion | 12.00 Overview, 12.01 Motion Tokens, 12.02 Controls, 12.03 Slider, 12.04 Dropdown |
| 14 Localization | 14.01 Languages, 14.02 Localized Screens |
| 20 Settings | 20.00 Overview, 20.01 General, 20.02 Dictionary, 20.03 Prompts, 20.04 Local Models, 20.05 Cloud Models, 20.06 ASR Connection JSON, 20.07 History, 20.08 Stats, 20.09 Status, 20.10 Sidebar |
| 21 HUD & Menu Bar | 21.00 Overview, 21.01 States, 21.02 Recording, 21.03 Errors, 21.04 Menu Bar, 21.05 HUD Motion Tokens, 21.06 HUD Processing, 21.07 Width & Pop-ups, 21.08 Follow Pointer, 21.09 HUD Buttons |
| 22 Onboarding | 22.00 Overview, 22.01 Welcome, 22.02 Permissions, 22.03 How to Use, 22.04 Shortcut, 22.05 Engine, 22.06 Cloud, 22.07 Try It, 22.08 Page Slide, 22.09 Usage Demo |

### Components and icons

- Anything drawn more than once is a component, and screens use instances of it. Shared components live on page `11`; parts used by one screen live in that screen's section.
- Main components sit directly in a named frame, not wrapped one per cell, because the Assets panel builds its folders from the frames around a component. Labels go next to them as sibling text.
- Icons live in `11.02 Icons`, one component per SVG in `crates/dito-app/assets/icons`, named after its path (`lucide/book-open`, `hud/finish`, `appearance/dark`). The description names the code that draws it.
- Size an icon instance with the scale tool (K) to the size the code passes, so the stroke scales the way GPUI draws the SVG. Change its color as an override on the `glyph` layer.
- When code adds, renames or removes an SVG, update its component in `11.02` in the same change.

### Controls

`11.03 Controls` holds the shared controls; screens use instances of them, never copies.

| Set | Properties | Notes |
| --- | --- | --- |
| `Button` | Kind (Primary, Secondary, Danger, Download, Outline), Size (Regular 36, Large 40, Hero 50), State (Rest, Hover), Label, Leading icon, Trailing icon, Icon, Trailing | Regular is for Settings, Large for onboarding, Hero for the welcome step. A Secondary button on a page background binds its fill to `card` instead of `input_bg`. Download is the blue model download button; Outline is the model card's 使用する |
| `Icon Button` | State (Rest, Hover, Active), Icon | 36 round, 16 pt icon |
| `Switch` | Value (On, Off) | 42 × 24, 18 knob |
| `Radio` | Value (On, Off) | 18, ring style when on |
| `Select` | State (Rest, Hover, Open), Label | Width is set per instance |
| `Segment`, `Segmented` | State (Selected, Unselected, Hover), Label; the `Segments` slot | Put `Segment` instances in the slot of a `Segmented` track. Each option is as wide as its label in Medium plus 16 on each side, so selecting never changes the width |
| `Appearance Control` | Value (System, Light, Dark) | The icon-only appearance switcher |
| `Slider` | State (Rest, Hover, Pressed), Start label, End label | 220 × 24 track, 24 thumb (26 on hover, 32 pressed). Set the thumb position and selected width per instance |
| `Menu`, `Menu Item`, `Menu Separator` | Menu Item: Tone (Default, Danger), State (Rest, Hover), Label, Leading icon, Icon, Check; Menu: the `Items` slot | One menu style for selects and action menus |

Every control's motion is a table in `12.02` (buttons, selects, segmented, switch, sliding plate), `12.03` (slider) and `12.04` (dropdown and menu): trigger, change, timing, interruption and the Reduce Motion fallback.

- Connection forms (`20.05`) use the form field: 40 high, radius 8, `field/bg` with a `field/border` hairline, captions 13 Regular in `muted`. Other screens keep the 36 pt field with radius 12.
- Dialogs (`20.06`): 28 side padding; top and bottom padding 14 in a short window (under 700 high) and 28 otherwise. The title row is 32 high and the 32 pt close button sits flush at its right end, so the cross is 38 from the right edge and 24 from the top in a short window (a golden-ratio pair), 38 from both in a tall one.
- A disabled control is the same instance at 45% opacity.
- After swapping an icon through a property, check its size and stroke: a swap brings the icon in at its own size and keeps the old stroke weight. A scaled icon's stroke is 2 × size ÷ 24.
- These specs are the design's and the code follows them; differences still open in code are listed in the change that introduced them.

### Color follows the surface

- Content color comes from the surface it sits on: dark content on light surfaces, light content on dark ones. This is done with the `Color` mode, not with separate light and dark components.
- Every screen frame sets the `Color` mode (Light or Dark). Components bind their fills, strokes, rings and shadows to `Color` variables, so one component draws correctly in either theme. Never make a Light and a Dark copy of a component.
- Content on a tinted surface uses the matching `*_text` variable (`success_text` on `success_soft`, `danger_text` on `danger_soft`); content on a solid primary or accent fill uses `on_primary`.
- A component that brings its own background owns both colors. `21.09 HUD Buttons` (Kind: Cancel, Finish, Dismiss, Retry, Settings; State: Rest, Hover) and `Close Button` on `20.06` (State: Default, Hover) are the reference.
- A hex value on an instance is allowed only where no variable matches yet; it is a gap to close, not a style.

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
- Section: radius 64 with 60% corner smoothing, holding its board 40 in from every edge so the corners stay concentric.
- Board: white, radius 24 with 60% corner smoothing, padding 64, auto layout. Top-left: number chip (Gold 05), title (32), one-line purpose (15, muted).
- Corner smoothing is for sections and boards only, because they are canvas, not product UI. Screen frames and everything inside them keep plain radii, since GPUI draws only circular corners.
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
| `Color` | Light / Dark | Every `Palette` field in `crates/dito-app/src/theme.rs` under the same name (`bg`, `text`, `muted`, `on_primary`, `seg_on`, `menu_bg`, …), the constants `white` and `plain_black`, and `hud/*` for each `hud_color` pair in `crates/dito-app/src/hud` plus `hud/danger_ring` and `hud/button_shadow` |
| `Motion` | — | Shared: `motion/hover` 160, `tip` 150, `ring` 180, `sidebar` 260, `switch` 380, `glass` 460. HUD and onboarding: `pill` 150, `hud-pop` 420, `hud-widen` 300, `hud-label` 120, `onboarding-slide` 360 (ms) |
| `Size` | — | `radius/input` 12, `radius/card` 16, `radius/window` 20, `window/width` 1120, `sidebar/width` 232 |

Icon instances, HUD buttons, the close button and the `11.03` controls are bound to variables; other existing layers still carry hex values. New components bind from the start, and a variable's values change together with the code that defines it.

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
