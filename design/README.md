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
Board    NN.MM Name       one top-level frame, e.g. 21.02 Recording
Item     NN.MM.K Name     one row group inside the board, e.g. 22.02.1 Microphone granted
```

| Range | Group | Pages |
| --- | --- | --- |
| 00–09 | Guide | `00 Cover & Index`, `01 Conventions` |
| 10–19 | `- Foundations` | `10 Color`, `11 Type, Space & Components`, `12 Motion`, `13 Dot Motif`, `14 Localization`, `15 Build Constraints` |
| 20–29 | `- Screens` | `20 Settings`, `21 HUD & Menu Bar`, `22 Onboarding` |
| 30–39 | `- Brand` | `30 App Icon` |

Section numbers follow visible order; remove retired sections and close their gaps.

The numbered section is normally the board itself, not a Section node wrapped around a same-named frame. Use a transparent Section only to group multiple related boards, such as the Light and Dark motif boards. Rows, cells and other layout-only frames have no fill or stroke. Keep wrappers only when they own layout, clipping, theme, behavior or a meaningful group.

### Section map

| Page | Sections |
| --- | --- |
| 10 Color | 10.01 Color, 10.02 Semantic Color |
| 11 Type, Space & Components | 11.01 Type, Space & Components, 11.02 Icons, 11.03 Controls, 11.04 Token Reference, 11.05 Icon System |
| 12 Motion | 12.00 Overview, 12.01 Motion Tokens, 12.02 Controls, 12.03 Slider, 12.04 Dropdown |
| 14 Localization | 14.01 Languages, 14.02 Localized Screens |
| 20 Settings | 20.00 Overview, 20.01 General, 20.02 Dictionary, 20.03 Prompts, 20.04 Local Models, 20.05 Cloud Models, 20.06 History, 20.07 Stats, 20.08 Status, 20.09 Sidebar |
| 21 HUD & Menu Bar | 21.00 Overview, 21.01 States, 21.02 Recording, 21.03 Errors, 21.04 Menu Bar, 21.05 HUD Motion Tokens, 21.06 HUD Processing, 21.07 Width & Pop-ups, 21.08 Follow Pointer, 21.09 HUD Buttons, 21.10 Stop Motion Preview, 21.11 HUD Elastic Motion Preview |
| 22 Onboarding | 22.00 Overview, 22.01 Welcome, 22.02 Permissions, 22.03 How to Use, 22.04 Shortcut, 22.05 Engine, 22.06 Cloud, 22.07 Try It, 22.08 Page Slide, 22.09 Usage Demo |

### Components and icons

- Anything drawn more than once is a component, and screens use instances of it. Shared components live on page `11`; parts used by one screen live in that screen's section.
- Main components sit directly in a named frame, not wrapped one per cell, because the Assets panel builds its folders from the frames around a component. Labels go next to them as sibling text.
- Public UI icons live in `11.02 Icons` as one shared 24 by 24 `lucide/*` component library. Controls and screens reuse these masters. Private animation and illustration fragments live with their screen and are hidden from publishing. Native application artwork is cataloged separately.
- Size an icon instance with the scale tool (K) to the size the code passes, so the stroke scales the way GPUI draws the SVG. Change its color as an override on the `glyph` layer.
- After design approval, keep exported SVGs and their shared components synchronized in the implementation change.

#### Lucide source and optical system

`11.05 Icon System` defines the design target for shared circular controls and HUD glyphs. Its canonical masters live directly in the `Lucide` frame on `11.02`. There is no separate HUD icon library or duplicate size catalog. HUD action and outcome assets are exported from these masters. Other application controls follow their approved section when migrated.

- Use [Lucide outline](https://github.com/lucide-icons/lucide) by default. Use [Lucide Extended](https://github.com/applifted/lucide-extended) for an intentionally filled or duotone variant, after checking its catalog. Extended supplements the outlines and is not an upstream Lucide project. Do not assume an icon has every style.
- When no icon fits, build a local variant from Lucide primitives. Use a 24 by 24 canvas, 2 pt centered strokes, round caps and joins, at least 1 pt of stroke-inclusive safe zone, and at least 2 pt between distinct strokes when drawing (keyline scaling may compress an existing master to 1.5 pt, never further). Larger right-angle corners use radius 2; small details use radius 1. Preserve recognizable shared geometry and avoid abrupt cuts where shapes overlap.
- A 24 pt viewBox is the placement contract, not a promise that every icon has identical visible bounds. Compare visual weight against circle and square. Dense icons, diagonal crosses and asymmetrical arrows need different optical treatment. Keep symmetry centered; bake approved asymmetric corrections into the contour instead of adding offsets at every call site.
- Keylines: every master in `11.02 Icons` follows one sizing rule on stroke-inclusive ink in the 24 pt canvas, so icons from different sources read as one set inside a circle. Upstream Lucide draws rings 22 pt wide and X 14 pt wide, which looks mismatched. The rule is automated by `.agents/skills/icon-keylines`, which runs whenever an icon is added or its shape changes.
  1. Marks keep their own boxes: X and plus 16 pt, arrows 16 pt, check 18 by 13 pt, chevron 14 by 8 pt, vertical dots 4 by 18 pt.
  2. Every other icon, a body, scales about the canvas center until the convex hull of its ink is about 254 pt² (circle 18, square 16), with a side of about 18 pt (18.4 for a ring, because the fixed 2 pt stroke is added to the scaled centerline). A body within 8 percent of the target is left alone.
  3. Density floor: the stroke never scales, so a body stops where a gap reaches 1.5 pt. Dense icons (clock, info, cpu, sun, sparkles, triangle-alert, mic) stay at 19.6 to 20 pt. `check:design` rejects any master with an ink side above 20 pt or a center more than 1 pt off the canvas.
  4. Optical center: put the mean of the bounding-box center and ink centroid on the canvas center. Drop shifts under 0.5 pt, cap the rest at 1 pt, and give marks none. Check keeps its approved 1 pt drop.
  5. Bake the result into the master contour, never into call sites. Corner radii scale with the contour and the stroke stays 2 pt. Copy is two 11 by 11 pt sheets 6 pt apart, so its 2 pt gap survives.
- Check shifts its Lucide centerline down 1 pt in the canonical 24 pt canvas. Stop keeps the 24 pt canvas, with a centered 14 by 14 pt square contour at (5, 5), 2 pt corner radius and the same 2 pt stroke. Scale both corrections with the instance canvas. Reset inherited contour size and position overrides so future master edits propagate; preserve semantic foreground overrides.
- Every HUD button uses a fixed 24 pt circle and a 14 pt icon slot, regardless of action or state. Choose the shared icon through the control template instead of selecting a separately sized master. Circular controls use Compact 24 with a 14 pt glyph canvas, Small 32 with 18, Regular 36 with 20, and Large 40 with 22. The glyph canvas is about 56 percent of the diameter, rounded to an even point size. These are Dito design choices, not Lucide requirements. The glyph is centered inside the circle; the visible ink stays inside its own canvas. Keep geometry stable across Rest, Hover and Active. Surface roles and elevation may differ by context.
- On an instance swap, recheck the glyph canvas and stroke. FigCraft may bring a replacement in at 24 pt or preserve an old frame size independently of its internal contour. Scale the contour from its canonical master, then explicitly restore the declared slot dimensions. Confirm both the frame and visible geometry, including after property changes.
- Every contour inside a master, including a group's children, uses Scale constraints on both axes. A replacement can already fit its destination canvas. Inspect its coordinates before using K; applying the slot scale again shrinks and displaces the contour a second time. Preserve semantic foreground overrides when rebuilding an instance.
- Stroke width follows the same ratio as the contour: a 2 pt master stroke becomes `2 * slot / 24`. Check nested parts as well as the outer instance. Update instance-swap properties when retiring a master so they cannot restore a deleted reference.
- After saving icon changes, run `mise run check:design`. It reads the saved file through `figcraft-cli`, checks every shared icon instance against its canonical contour, and rejects changed proportions, misplaced parts, wrong stroke widths, incorrect circular-control slots and missing component references. It also checks vector vertices and curve handles. This is a geometric check; native-size visual review still determines optical balance, color and motion.
- Copy uses the horizontally mirrored Lucide construction: left sheet in front, right sheet behind. Mirror the full construction before extracting front and back, bake the reflection into coordinates, and keep the two extracted SVGs on the same 24 pt canvas. Preserve the 2 pt inter-layer gap. Draw the rear first and the front last; appearance timing is independent of paint order.
- Future import tooling should rasterize transparent masters, measure stroke-inclusive bounds, alpha-weighted centroid, occupancy and connected regions, then suggest a small contour scale and translation. A reviewer compares the result at native size in both themes before the accepted correction is baked into the SVG. Measurements must not force equal bounding boxes or automatically claim perceptual equality. This is the import-tool specification; automated tooling is not implemented by this design-only change.
- HUD circular buttons reuse the shared Lucide assets at 24 / 14 pt. HUD outcomes use an 18 pt Check and a 14 pt Copy contour, both centered on an 18 pt line. Judge Copy by the full union of its two sheets, not by either sheet alone. The animated check follows the shared 24 pt centerline, and Copy front, back and settled contours use the same 24 pt canvas. Re-export these assets together when changing their master. Shipped icon notices live in `crates/dito-app/assets/icons/LUCIDE-LICENSE.txt`.
- Completed insertion uses `hud/on_inserted` (constant white); copied outcomes use `hud/on_copied`. These roles apply to the floating HUD, onboarding and all outcome storyboards. Check uses an 18 pt canvas and a 1.5 pt stroke. Copy uses a 14 pt canvas and a 1.167 pt stroke, with both private animation fragments on that same canvas; the recording Finish action uses the separate 24 / 14 pt button contract.
- A check reveal keeps its complete 18 pt canvas centered throughout. Reveal length follows `(3, 9.75)`, `(6.75, 13.5)`, `(15, 5.25)` in that canvas. At 140 ms of the 320 ms reveal the visible prefix ends at `(8.25, 12)`. Do not center or resize that prefix by its own bounds. Copied labels use an 18 pt line box centered on the capsule, matching the icon canvas; animated characters use the same settled baseline. `check:design` verifies these declared outcome contracts as well as the shared geometry.

Sources: [Lucide design language](https://lucide.dev/contribute/icons/design-principles), [Lucide specification](https://lucide.dev/contribute/icons/specification). Visual references: [Fyler](https://dribbble.com/shots/27577538-Design-System-for-Fyler) for circular glyph proportions and [MingCute buttons](https://dribbble.com/shots/27499879-Light-Dark-Filled-Ghost-Icon-Buttons-in-One-Figma-System) for state organization. These references inform the design; their exact dimensions are not inferred from presentation videos.

Copied and adapted icon geometry retains the upstream notices in [Lucide license](icon-licenses/LUCIDE.txt) and [Lucide Extended license](icon-licenses/LUCIDE-EXTENDED.txt). Preserve the notices when exporting or distributing assets.

### Controls

`11.03 Controls` holds the shared controls; screens use instances of them, never copies.

Text buttons and icon-only circles use separate icon contracts. Regular text buttons are 36 pt high with a 16 pt leading icon and a 14 pt trailing icon; Large uses 40 / 18 / 16 pt; Hero uses 50 / 20 / 18 pt. The icon-to-label gap is 8 pt. Horizontal padding is 16, 22 and 40 pt, respectively. Width hugs the actual label, visible icon slots, gaps and padding unless the screen explicitly requires a full-width action. Recalculate after copy changes instead of retaining an obsolete minimum width. The Prompt actions `アプリを選ぶ` and `変更` now use natural widths of 131 and 73 pt at the Large preset.

Icon-only actions use Icon Button, including model use, download, pause, retry, cancel and more actions. Do not make a circle by hiding a Button label. Circular diameter / canvas presets remain 24 / 14, 32 / 18, 36 / 20 and 40 / 22 pt. HUD actions remain 24 / 14 and Close remains 32 / 18. Native app artwork, generic app identities, menu checks, select chevrons, inline help and compact media actions have their own declared patterns. `11.05.6 Control icon contracts` records the complete matrix. Each outline stroke is `2 * canvas / 24`; preserve the same canvas and center across Rest, Hover and Active. The saved-file check rejects wrong control families, sizes and centers as well as distorted SVG geometry.

Each catalog item has a numbered title, purpose, main-component matrix, Light and Dark instance examples, and an aligned Notes card. Variant labels sit beside the mains, outside their product geometry. Button kinds and sizes form rows, with Rest and Hover in columns. Disabled and icon examples use instances. The source column explicitly uses Light mode; preview frames own their Light or Dark mode. Keep the component IDs and property bindings when reorganizing a catalog.

`11.01` links to the catalog instead of keeping a second set of illustrative controls. `11.04 Token Reference` documents the reusable text and effect styles, spacing, radius and component anatomy.

Tracking follows the text's role. Reading content, captions, field values, select values, menu items, application names and model names use natural tracking (0%). Names keep natural tracking even when they appear inside a control. Body text does not inherit a button's 5% tracking. Headings retain their existing display tracking, and the DITO wordmark retains its dedicated brand spacing.

Control labels keep the actual design face: Medium (500) for buttons and selected segments, Regular (400) for unselected segments. Japanese and Chinese action buttons and segmented buttons retain 5% tracking at the regular size, 4% for large and hero buttons; Latin-only labels use natural spacing. Segment Regular has its own text style, separate from Menu. Fit segment widths to the label at both weights so selection does not resize the control. The native app disables additional color-dependent macOS stroke dilation in its process-local volatile preferences before GPUI initializes. Do not compensate for renderer differences by changing the design weight.

Menus use 34 pt item rows and a fixed 2 pt gap. A Menu Separator is a transparent 9 pt layout row containing 4 pt top space, a 1 pt rule and 4 pt bottom space. It never paints a border on a hovered item. The same 2 pt gap appears before and after the separator, keeping the rule 6 pt clear of each adjacent item's background. The pinned Add application action and the scrolling application list follow this same spacing. Application-menu identities use a fixed 24 pt slot: native artwork fills that slot, while the centered Lucide canvas is 20 pt with a 1.667 pt stroke. Names use natural 14 pt type with a 20 pt line box and a 10 pt icon-to-label gap. Add application and Other use the shared `application/add` and `application/other` templates, keeping generic line art from looking larger than native artwork.

Closed Application Select controls use a different, explicit contract: control height 36 pt, identity slot 20 pt, native artwork 20 pt, generic Lucide canvas 18 pt centered with a 1.5 pt stroke, chevron 14 pt with a 1.167 pt stroke, label 14 pt with a 20 pt line box, icon-to-label gap 10 pt. Never insert a raw 24 pt Lucide master into an application identity slot. The geometric design check rejects mismatched slots, missing identity templates and incorrectly sized or centered nested glyphs.

This is the shared menu composition contract, not an application-picker exception. Each child type owns its height: an option, separator, heading or custom content does not have to match another type. The parent measures the actual children, then adds the standard gap and outer padding. Only Menu Item instances use the 34 pt option height.

The approved Prompt editor lives in `20.03 Prompts`. Shared rules, app rules and preview panes use minimum heights of 168, 144 and 135 pt, respectively, about 1.5 times their former heights. A single 36 pt header puts the title and save state at the left, with history actions grouped at the right. The 200 pt application selector is last; the optional Delete action uses the same neutral foreground as the other actions. The action group hugs its content, with no empty slot when Delete is hidden, and always anchors to the selector's left. Toolbar circles use the shared Small template, 32 pt with an 18 pt icon; Try it uses a 16 pt icon. The page scrolls inside the existing native viewport.

Other is the fallback template for applications without a dedicated profile, not an empty application selection. `20.03.1` shows this state with three right-aligned history actions. `20.03.7` shows opening and selected states side by side in both themes. The selection region is derived from the shared Large button's 40 pt height: artwork and the name line box are also 40 pt, vertical insets are 16 pt, and the enclosing surface is 72 pt high. The natural 25 pt Medium app name and 16 pt identity gap emphasize the selection above the smaller copy-source selector. The Large action hugs its label, 131 pt for Choose app and 73 pt for Change in Japanese, and sits inside the same surface at the identity's vertical center.

On open, the left side shows only `未選択`, with no app icon or duplicate instruction. The single action reads `アプリを選ぶ`, and Add is disabled. A native picker result replaces the status with app artwork and name, changes the action to `変更`, and enables Add. Choosing alone does not create the profile; cancelling the picker preserves the previous state. The action labels have no ellipsis. These labels and the empty status are localized in `messages.toml`.

Each editor owns Undo, Redo and Restore defaults for its current language and target. Draft autosave after 3 s idle is separate from meaningful history checkpoints. A 2 s pause or leaving the editor closes a group; tiny corrections coalesce rather than creating per-character versions. Preserve the pre-edit baseline and IME composition boundaries. Every Restore defaults action opens a confirmation naming its scope, then creates one undoable replacement only after confirmation. App rules and optional writing examples form one app group. The bottom Save and global Reset row is retired. Preview stays plain text, editable input on the left and read-only output on the right, without history controls or diff markup. Japanese language names follow `Lang::name(ui)`, including `簡体字中国語`.

| Set | Properties | Notes |
| --- | --- | --- |
| `Button` | Kind (Primary, Secondary, Danger, Download, Outline), Size (Regular 36, Large 40, Hero 50), State (Rest, Hover), Label, Leading icon, Trailing icon, Icon, Trailing | Regular is for Settings, Large for onboarding, Hero for the welcome step. A Secondary button on a page background binds its fill to `card` instead of `input_bg`. Download is the blue model download button; Outline is the model card's 使用する |
| `Icon Button` | Size (Compact, Small, Regular, Large), State (Rest, Hover, Active), Icon | Circle and glyph canvas: 24/14, 32/18, 36/20, 40/22 pt. Shared optical rules: 11.05. The application migration follows design approval |
| `Status Label` | Tone (Success, Warning, Danger, Neutral), Label | 24 pt solid inverse pill, 10 pt horizontal padding, 12 pt Medium text with an 18 pt line. No dot. Status cards and permissions use the same component. Success uses `success` with `white`, Danger uses `danger` with `white`, Warning uses `warning_text` with `warning_soft`, Neutral uses `muted` with `card` |
| `Context Help` | State (Rest, Hover) | 24 pt hit area, 16 pt inverse information disc in `text` and `card`. Opacity 60% at rest, 100% on hover or keyboard focus, `motion/hover` 160 ms ease-out. Tooltip uses `motion/tip` 150 ms and the established shadow and radius; its first entrance rises and fades, while rapid changes reuse the window's bubble. Allow 120 ms between targets, then fade out in 150 ms. Position and width transfer in 180 ms and retarget from the current position and velocity. Keep labels centered on one line, cross-fade them, and apply a 6 pt alpha fade to overflowing left and right edges. Reduce Motion updates position and width immediately. The storyboard and interactive preview are on 12.05 Tooltip Travel |
| `Application Select` | Label, Icon, Leading icon | Standard 36 pt select, radius 12, `input_bg`, `border_strong` and `text`; 20 pt native app icon. Use a centered 18 pt Lucide grid for `その他`. Its menu keeps Add application above the scrolling installed-app list |
| `Local Source`, `Cloud Source` | Label | 24 pt outlined capsule, 14 pt CPU or globe icon and 12 pt Medium label. Neutral `card`, `border_strong` and `muted` colors. Place immediately after the model name with a 10 pt gap; no button interaction |
| `Status Detail` | Label, Value | Two equal columns in model slots on `20.08`. Values wrap; each row grows to the taller tile. Default height 64 pt, 16 pt horizontal and 10 pt vertical internal padding, 12 pt gutter. Both detail tiles and model cards use `radius/card` (16 pt). Align tile edges with the slot heading and model-name text, 22 pt from the model card edge. Status permission rows use 18 pt vertical padding |
| `Switch` | Value (On, Off) | 42 × 24, 18 knob |
| `Radio` | Value (On, Off) | 18, ring style when on |
| `Select` | State (Rest, Hover, Open), Label | Width is set per instance |
| `Segment`, `Segmented` | State (Selected, Unselected, Hover), Label; the `Segments` slot | Put `Segment` instances in the slot of a `Segmented` track. Each option is as wide as its label in Medium plus 16 on each side, so selecting never changes the width |
| `Appearance Control` | Value (System, Light, Dark) | The icon-only appearance switcher |
| `Slider` | State (Rest, Hover, Pressed), Start label, End label | 220 × 24 track, 24 thumb at rest and on hover, 32 pressed. Rest has no shadow; hover changes the thumb fill and reveals its soft shadow over 160 ms, without growth. Release returns the thumb to 24 pt. Set the thumb position and selected width per instance. Advance springs on display animation frames while unsettled, with no idle timer. Paint thumb growth and shadow inside a fixed 24 pt layout and hit area; Reduce Motion keeps the resting size. Draw the selected fill as one shape with rounded left corners and a straight end at the thumb center; overlapping cap and rectangle layers expose a seam during page fades. Fade a hiding row with one card-colored overlay over the opaque control, preserving its normal internal compositing |
| `Menu`, `Menu Item`, `Menu Separator` | Menu Item: Tone (Default, Danger), State (Rest, Hover), Label, Leading icon, Icon, Check; Menu: the `Items` slot | One menu style for selects and action menus |

Model-slot headings on `20.08` use an 18 pt microphone or sparkles icon in `text`, followed by an 8 pt gap and the 16 pt Bold title. Keep icons vertically centered in the 24 pt heading line. The model-name row has a 2 pt optical inset relative to the heading icon and detail-card edge; keep its source badge 10 pt after the name.

Model-status popovers keep unfilled 12 pt text followed by a 6 pt status dot, with a 6 pt gap. Align the dots to the right edge across rows. Labels use `muted`; dots use `success` for running, `warning_text` for loading or stopping, `muted` for configured, stopped or not installed, and `danger` for errors. Configured means saved settings, not a verified connection. Bind the appearance mode explicitly in standalone examples. Runtime status stays distinct from the filled role labels in the same row.

Every control's motion is a table in `12.02` (buttons, selects, segmented, switch, sliding plate), `12.03` (slider) and `12.04` (dropdown and menu): trigger, change, timing, interruption and the Reduce Motion fallback.

Documentation tables own one 8 pt rounded outer frame with a 1 pt inside border and clipped contents. Header and body rows fill the same inner width, have square corners, and use only bottom separators; the last row has no separator. Keep column widths and padding identical between header and body. The board must contain the full table plus its padding. After widening a board, preserve the 240 pt gap to neighboring boards.

- Connection forms (`20.05`) use the form field: 40 high, radius 8, `field/bg` with a `field/border` hairline, captions 13 Regular in `muted`. Other screens keep the 36 pt field with radius 12.
- Cloud speech configuration (`20.05`) uses the complete inline JSON editor. The template dropdown loads reference configuration directly into it. Keep API keys in their separate secure field, language-code shortcuts and circular Undo, Redo and Copy above the editor. Undo and Redo use curved 20 pt glyphs; Copy uses an 18 pt glyph on the standard circular button surface. Help uses a 12 pt glyph. The editor and validation box share a soft `border` hairline, radius 12 and `sub` surface. Validation has no separate heading. Below it, show connection status and latency on the left, Test connection and Help on the right.
- Valid drafts autosave after a 200 ms debounce. There is no Save action. Invalid drafts retain the last valid saved configuration and disable testing. Keep separate drafts and native undo history per template; language-code shortcuts must also be undoable. Connection tests use the current validated draft and ignore stale results after draft or language changes.
- Help opens the sole documentation dialog. Remove the retired four-step strip and present the four reference cards directly. Keep the title, close control and bottom Copy instructions for AI action fixed. Only the four-card region scrolls in a short window; clip both fades to that region. Preserve light and dark top, middle and bottom examples in `20.05.4`.
- `20.05` also contains validation, compact-window and editor scroll-end examples, merged from the former Cloud Configuration Details section. The settings page scrolls vertically; the 360 pt JSON editor has its own viewport and edge fades. Do not scroll beyond the document: set `scroll_beyond_last_line(Some(0))`, leave only normal content padding, and hide the lower fade at the end.
- A disabled control is the same instance at 45% opacity.
- After swapping an icon through a property, check its size and stroke: a swap brings the icon in at its own size and keeps the old stroke weight. A scaled icon's stroke is 2 × size ÷ 24.
- Match icons by visible contour, stroke weight and optical centre, not only SVG viewBox or container dimensions. Compare neighbouring buttons at actual size in both themes; adjust glyph scale, position and stroke where needed so no icon looks smaller, heavier or off-centre. Keep the button hit area unchanged. Shared corrections belong in `11.02 Icons`; context-specific corrections belong to the screen's icon instances and must be carried into the implementation.
- These specs are the design's and the code follows them; differences still open in code are listed in the change that introduced them.

### Color follows the surface

- Content color comes from the surface it sits on: dark content on light surfaces, light content on dark ones. This is done with the `Color` mode, not with separate light and dark components.
- Every screen frame sets the `Color` mode (Light or Dark). Components bind their fills, strokes, rings and shadows to `Color` variables, so one component draws correctly in either theme. Never make a Light and a Dark copy of a component.
- Content on a tinted surface uses the matching `*_text` variable (`success_text` on `success_soft`, `danger_text` on `danger_soft`); content on a solid primary or accent fill uses `on_primary`.
- A component that brings its own background owns both colors. `21.09 HUD Buttons` (Kind: Cancel, Finish, Dismiss, Retry, Settings; State: Rest, Hover) and `Close Button` on `20.05` (State: Default, Hover) are the reference.
- A hex value on an instance is allowed only where no variable matches yet; it is a gap to close, not a style.

Dictionary settings on `20.02` use one shared word list without a language selector or filter. The add form contains a flexible word input and a right-aligned 36 pt Add button, with a 12 pt gap and bottom-aligned input and button. Enter adds the word; a successful insertion clears the input and refreshes the list, while blank or duplicate words add nothing. Show the unsupported-hotword notice only for a selected local ASR model without hotword support. Normal and loading examples use the same form, and loading does not disable editing, search or the polishing option.

Prompt settings on `20.03` show open shared rules first, app-specific writing rules second, and a side-by-side preview third. Keep both editing titles visible. The text-language picker sits at the top right. Put Add application in a pinned row at the top of the dropdown and scroll only the app list. List built-in profiles only for detected installed apps, use their native icons, and keep `その他` as the fallback. Adding a profile selects a macOS app and an existing application profile to copy. Copy its writing instructions and style examples independently across languages, keep common rules unchanged, and use the native app name without a display-name field. Preserve saved profiles when an app is temporarily absent. Herdr instructions clarify intent without inventing requirements or executing dictated instructions. Optional examples guide style without adding their content. Use Context Help for shared rules and optional examples, with tooltips centered above the icon and clamped to the window. App-specific settings and the preview title explain themselves without help icons; the preview title is `整形を試す`. The polish toggle stays in General. Preview uses unsaved edits without typing elsewhere. Restore defaults confirms its editor and language scope, then replaces only that editor with one undoable change. The implementation preserves saved legacy profiles and stores custom app rules separately for each language.

Settings sections follow the app's navigation order. The prompt editor matches gpui-kit's Textarea: 13 px text, 20 px lines, no letter spacing. Older numbers (`03A`, `10c`, `20.0x` from the short-lived Japanese layout) are retired; code comments cite the new numbers or token names.

## 3. Naming

| Object | Format | Example |
| --- | --- | --- |
| Page | `NN Name` | `22 Onboarding` |
| Board | `NN.MM Name` | `21.02 Recording` |
| Item title | `NN.MM.K Name` | `20.01.3 HUD height slider` |
| Screen frame | `Screen (theme)`, or `Screen (theme, language)` when not Japanese | `General (dark)`, `General (light, English)` |
| Keyframe caption | `elapsed` or `elapsed: state` | `190 ms: about 108 pt` |
| Component layers | `kind-name` in English kebab-case | `btn-primary-save`, `nav-general`, `hud-capsule` |

Layer names derived from UI copy use the English string of the same i18n key. About 400 deep layers whose copy has no key still carry Japanese; rename them when touched.

## 4. Layout

- Screen pages and `12 Motion` start with `NN.00 Overview` at the far left (title, purpose, section list).
- Boards run left to right in number order, top-aligned, 240 apart. The layer tree follows the same order.
- Each board is one top-level frame owning its background and content layout. Do not add a same-named Section or decorative frame around it. A multi-board Section is transparent and has no border or decorative radius.
- Board: white, radius 24 with 60% corner smoothing, padding 64, auto layout. Retain a distinct presentation background where the examples require it. Top-left: plain section number (12, gold), one title (32, without the section number), one-line purpose (15, muted). Category pills such as Foundation are unnecessary because the page already provides that context.
- Corner smoothing is for documentation boards only, because they are canvas, not product UI. Screen frames and everything inside them keep plain radii, since GPUI draws only circular corners.
- Item: title (20), optional one-line note, then rows. Each frame sits in a cell with a small caption above it.
- Light on the left, dark on the right. When a row holds only one theme, a small gold "Light" or "Dark" label sits above the row and the captions drop the theme.
- Captions are 3 to 5 words. Longer explanation goes in the Notes card.
- Storyboards run left to right in time order.
- A note card (360 wide, 1–3 lines) closes the first row of an item on the right. It sits under an outlined gold "Notes" tag and stretches to the row's height, so its top and bottom line up with the frames beside it. The card is white with a 1 px Gray 02 border and radius 16. Never let a note float alone in a corner.
- Documentation styling comes from this file's existing boards and palette (outlined gold tags, white cards with a Gray 02 border, Gold headings). Don't bring in generic web patterns such as colored side stripes or filled badges.
- Nothing floats on the canvas.
- Background bounds must follow content changes. After adding, removing or resizing cards, labels or examples, inspect every affected ancestor up to the board. Let auto-layout boards hug their content; resize fixed presentation boards explicitly. A multi-board group must still contain its boards, without adding a second painted surface.
- Verify the entire board, including its bottom edge, after edits. No card, note or example may extend beyond its documentation board. Inspect solved bounds and render the whole board; checking only a selected screen or component is insufficient.
- Give isolated control and tooltip specimens at least 16 pt of presentation space on every side, including below the last circular control. Keep structural specimen containers unclipped so antialiasing, outlines and shadows remain visible. This space belongs to the presentation container, not to the product button or icon. Preserve deliberate clipping in actual scroll views, masks and product surfaces. Recheck the enclosing board after adding space.

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
- Motion that belongs to one screen stays with it: HUD tokens and storyboards on 21.05–21.08 (Copied sits beside the check on 21.06), History hint on 20.06.3 and loading skeleton on 20.06.4, onboarding on 22.08–22.09.
- Screens cite a token name instead of restating shared timings.
- Every motion spec states: trigger, change, token (or duration + easing / spring), what happens if interrupted, and the Reduce Motion fallback (no stretch, overshoot, scale or travel; a short fade instead).

## 7. Tokens

| Collection | Modes | Content |
| --- | --- | --- |
| `Primitive` | Base | Raw gold, gray and deep-gold ramps, plain colors, signal colors and existing dark-surface values. Product layers bind to semantic `Color` roles, not directly to this collection. Palette swatches bind to their primitives |
| `Color` | Light / Dark | Every `Palette` field in `crates/dito-app/src/theme.rs` under the same name (`bg`, `text`, `muted`, `on_primary`, `seg_on`, `menu_bg`, …), the constants `white` and `plain_black`, and `hud/*` for each `hud_color` pair in `crates/dito-app/src/hud` plus `hud/danger_ring` and `hud/button_shadow` |
| `Motion` | Base | Shared: `motion/hover` 160, `tip` 150, `ring` 180, `sidebar` 260, `switch` 380, `glass` 460. HUD and onboarding: `pill` 150, `hud-pop` 300, `hud-widen` 300, `hud-label` 120, `onboarding-slide` 360 (ms). Other screen timings remain in their owning section |
| `Size` | Base | Existing window metrics and radii, named spacing, control heights, icon sizes, boundaries, `opacity/disabled` (45 percent) and `canvas/*` dimensions. Canvas tokens are documentation-only; product dimensions are points |
| `Type` | Base | `family/ja`, `family/latin`, `family/zh`, `family/fallback` and `size/*`. Text styles preserve weight, leading and tracking together |

`10.02 Semantic Color` shows the path from Primitive to Color to a main component to its instances, with paired surface and content examples in both modes. Equal palette values reference one primitive; semantic roles retain separate names even when their values currently match. Theme-specific and calculated colors keep their existing values. An alpha overlay is not replaced by an opaque tint.

Text styles live under `Product/JA`, `Product/Latin` and `Canvas`. Main control labels and the type specimens use these styles, with family and size bound to `Type`. Locale-specific instance overrides remain valid. The effect styles `Elevation/Control Knob`, `Elevation/Slider Hover`, `Elevation/Slider Pressed` and `Elevation/Menu` preserve the existing shadow geometry and bind their color to the matching Color role.

Icon instances, HUD buttons, the close button and the `11.03` controls are bound to variables; other existing layers still carry hex values. New components bind from the start. `circle/*` and `icon/canvas`, `icon/stroke`, `icon/safe-zone`, `icon/element-gap` are review-stage geometry targets from 11.05; existing runtime values are synchronized only after design approval.

## 8. Language in the product

- The app's first impression is Japanese: it is built for Japanese customers and users.
- Users can switch to English or 简体中文 (or follow the system) in General → 表示言語. Switching is instant across settings, HUD and menu bar.
- **Code gap:** `DisplayLanguage` currently defaults to `System` (`crates/dito-core/src/config/general.rs`), so a Mac set to English starts in English. The design calls for Japanese as the default.

Type: Japanese and mixed text use M PLUS 1p; Latin-only text uses Outfit; 简体中文 uses IBM Plex Sans SC; macOS fallback is Hiragino Kaku Gothic Pro.

## 9. Build constraints

The app is drawn in Rust with GPUI. Avoid, or check with engineering first:

- Unscoped background blur or frosted glass on in-app surfaces → opaque color plus one soft shadow. HUD Cancel and Finish have a scoped material target in `21.09`: 6 pt background blur, 90% surface tint at rest, 96% on hover, a 0.75 pt edge and fully opaque glyphs. Where scoped blur is unavailable, use 96% tint at rest and opaque hover. A low-opacity fill alone is not the approved material.
- Inner shadows, blend modes, free-form masks.
- Gradient, outlined or path text.
- Rotating arbitrary elements, 3D (SVG icons and images can rotate).
- Custom colors or radii in native menu bar menus.
- Moving the title bar or traffic lights.
- Motion without numbers.

Engineering background: `docs/ARCHITECTURE.md` and the ADRs in `docs/adr/`.

## 10. Workflow

1. Decide where the change belongs: a shared component (`11`, `12`) or one screen's section. Edit it there and nowhere else.
2. Add the section at the end of its page with the next number; build it from an existing board.
3. Copy comes from `messages.toml`; settle new copy in design before adding a key.
4. Shared component motion goes on `12 Motion`; screen-specific motion stays in that screen's sections.
5. Add annotations to interactive or animated layers.
6. Recheck content groups and boards. Keep layout-only frames transparent and resize each board to contain its current contents with the intended padding; shared-component boards are included.
7. Render light, dark and localized screens, then the entire affected section including its bottom edge. Verify solved bounds and check for clipping, overflow and backgrounds left at their previous size before saving.

### Shipped icon assets

`21.09 HUD Buttons` owns the Cancel and Finish surface material. The `hud/glass`, `hud/glass_hover`, `hud/finish` and `hud/finish_hover` roles carry the surface alpha; do not multiply it again with paint or group opacity. Instances inherit fill, effects and edge from the component, including listening previews and onboarding. The glyph remains a separate fully opaque layer. The native renderer uses the approved fallback of 96% tint at rest and opaque hover, with the 0.75 pt edge and opaque glyphs. Its procedural ocean spreads dots locally beneath the moving button circles; GPUI does not provide a native backdrop filter. This fallback is shared by the HUD and onboarding.

`crates/dito-app/assets/icons/lucide` contains the shared 24 pt exports from the approved masters. Normalize monochrome paint to `currentColor` after export, preserving geometry, caps, joins and the canvas. `icons/hud` holds the automatic-language mark and private 24 pt Copy animation fragments. `icons/illustrations` holds the narrow onboarding arrow and mouse-button illustration. Native app artwork comes from macOS. The `Ic` mapping is the rendering entry point; control presets select the final size. Do not add per-size SVG copies or page-specific optical offsets.

Prompt history belongs to its page, scoped by editor, language and application. Drafts save after 3 s idle and flush on leaving the editor or page. Meaningful checkpoints close after 2 s idle or an outside click; fewer than eight changed characters coalesce until an explicit action or a larger edit. Continuous edits checkpoint every 15 s. Each history keeps its initial baseline and at most 100 snapshots. Keyboard, context-menu and toolbar Undo and Redo share this history. IME marked text is neither persisted nor checkpointed. The Add application dialog contains only the target identity, one choose/change action, the copy-source selector and confirmation buttons. No display-name field or helper paragraphs. Application selectors use the same trigger, arrow animation and menu entrance as other selects. Clicking outside any focused input dismisses editing without swallowing the next click. New custom applications retain their creation template; legacy applications without one restore the built-in Other template only after confirmation. Removing a built-in profile disables its automatic match until the user adds that app again. Removing a profile never deletes history recordings.

HUD language labels use Outfit at 12 pt with a centered 18 pt line box. Keep the label center aligned with the selected Auto cell, including the compact two-option example. Shared labels inside controls use vertical middle alignment rather than a manually raised baseline. The outcome proof in 21.06.1 is reused in 22.09.2, so the onboarding reference and HUD reference cannot drift apart. The final Copy outcome uses a 14 pt canvas for both sheets together, as refined in 21.06.2; it is not two independent 14 pt squares. The native shared check master includes the approved 1 pt downward optical shift, and the Stop contour occupies the central 14 pt of its 24 pt master.

`21.07.5 Processing Stop hover` defines the HUD-only Stop disclosure. During Recognizing or Polishing, entering the capsule reveals the pill 6 pt above it, using `motion/pill` (150 ms ease-out), opacity 0 to 1 and a 6 pt rise. The processing capsule stays fixed. The pill, the capsule and the gap form one continuous hover zone. Leave it for 250 ms before starting a 150 ms fade and downward exit; keep the visual mounted until that exit finishes. Re-entry retargets from the current progress. Reduce Motion fades without translation. Bind the square outline and label to `hud/stop`, with the appropriate Light or Dark mode, rather than inheriting ordinary text ink. The native implementation uses the same retained hover disclosure and semantic colors, with a localized hit width and a 140 ms inversion on hovering Stop. `21.10 Stop Motion Preview` provides an automatic Light and Dark playback flow and a separate interactive screen. Space switches between the two modes. The timed frames reuse the local hover variants. The hovered pill does not issue a competing leave action: the combined hover zone owns dismissal, and the capsule return zone restores the shown appearance. This preserves the delayed exit when the pointer leaves the button and the whole zone together.

Shared UI implementation entry points are `select_trigger` for select chrome and arrow motion, `circle_control` for circular geometry and hover appearance, the shared button renderer for active and disabled actions, `modal_layer` for modal focus ownership, and the input focus helpers for entry and outside-click dismissal. Screen modules supply content, semantic appearance and actions. Add missing behavior there instead of reproducing these control foundations in a screen. The approved Stop hover is implemented through the shared hover clock and HUD semantic pill renderer.


### Coupled HUD spring geometry

`21.05.1 Coupled entry and rebound` and `21.05.2 Envelope and projected dots` define the new design target. `21.11 HUD Elastic Motion Preview` plays the Light and Dark samples together. Source poses and copied feedback reuse local components and shared HUD controls. The native HUD, inline style preview and onboarding demo share these approved entry poses, capsule renderer and outcome clocks.

Entry lasts 300 ms and has one rebound. At 0 ms, width is 48%, height is 52%, and the center is 12 pt below rest. At 80 ms, width is 76%, height is 116%, and the center is 3 pt above rest. At 150 ms, width is 103%, height is 105%, and the center is 1 pt above rest. At 300 ms, the pose is exact rest. Each channel returns monotonically after its first peak. Join the poses with continuous velocity, retain the current pose and velocity on interruption, and do not replay entry for audio or label changes.

Button scale follows 0.20, 0.72, 1.04 and 1.00 at those poses. Their centers spread out from the capsule center using the current width and button radius. Keep button circles, glyphs, stroke, blur and shadow uniformly scaled; never apply the shell's unequal X and Y scale to them. The geometry check accepts explicit `hudMotionScale` only as a multiplier of the canonical 24 / 14 pair, including enlarged review samples; ordinary controls keep their fixed presets.

The ocean uses the same current capsule rectangle. Project each live 3D point to normalized u and v, then use `X = left + u * W` and `Y = top + v * H + releaseLift(u)`. The release pose lifts the middle by up to 1.5 pt and the ends by 0 pt. Recompute centers and the rounded clip from the current pose, retain point identities and depth order, and keep radii circular. Audio phase and level continue independently of the entry envelope. The button frost masks follow the moving button centers and radii. FigCraft shows sampled geometry; the native renderer must perform the live projection each frame rather than stretching a finished bitmap.

Hold-to-full width changes from 72 to 108.2 to 105 pt over 300 ms with one rebound. Copied feedback measures the full localized label only to choose the final capsule target. It does not reserve hidden trailing text in the active content group. The shell grows continuously from 105 to about 127.9 to 126 pt for Japanese, with no second undershoot. Final text is vertically centered. Reduce Motion uses final geometry and a 100 ms fade.


### Ordered copy reveal and dynamic content centering

`21.06.2 Ordered copy reveal` is the source for the corrected sequence in `21.11`. The left/front sheet fades in during 0–120 ms. Only after it is fully visible does the right/rear sheet fade in during 120–220 ms. The first character starts at 240 ms, after both sheets are fully visible. These three phases do not overlap. Each sheet has its own opacity clock. Both retain the same 14 pt vector canvas; rear paints first and front paints last. Only the currently visible construction contributes to the active icon advance, from 10.5 pt for front-only to 14 pt when complete.

Characters begin at 240 ms, then enter individually from left to right, 26 ms apart. Each grapheme has its own local clock, opacity, vertical offset and layout advance. Each character starts 6 pt below the baseline with zero vertical velocity and follows a short underdamped spring. Use damping ratio 0.40 and angular frequency 43 rad/s for the initial motion. With wd = 43 * sqrt(1 - 0.40^2), its screen-space offset is y(t) = 6 * exp(-0.40 * 43 * t) * (cos(wd * t) + 0.40 / sqrt(1 - 0.40^2) * sin(wd * t)). This produces an approximately -1.52 pt first peak at 79.7 ms. From that zero-velocity turning point, use a monotonic cubic Hermite return with zero endpoint velocities, reaching exact 0 at 165 ms and holding it. The velocity stays continuous at the turning point, with no second rebound. Opacity still reaches 1 in 28 ms, while a visible part of the upward travel remains. This adapts the liquid-glass damped-rebound principle to a small text displacement; it is not the sliding plate’s longer travel or duration. The text container and baseline do not bounce. The copied label is 11 pt on an 18 pt line, with natural spacing. The Japanese label is `コピーした！`, English is `Copied!`, and Simplified Chinese is `已复制！`. The exclamation enters as the final independent grapheme on the same spring and stagger. Outside Japanese, measure natural grapheme advances instead of assuming equal widths.

At every frame, let `A` be the active icon advance, `g` the gap eased from 0 to 6 pt as the first character appears, and `a[i]` each revealed character's measured advance eased with its appearance. Then `G = A + g + sum(a[i])` and `groupLeft = (capsuleWidth - G) / 2`. Place the icon at this left edge and each character after its preceding advances, centered in its current advance column. Hidden trailing characters contribute zero width. The icon therefore moves left continuously as the label appears, while the active icon, whitespace and text remain centered together. Capsule width and content-group width are distinct values, and neither is updated in discrete character-sized steps.

The storyboard samples and native copied-label renderer use these separate clocks and active widths. Glyph opacity and active advance follow `1 - (1 - clamp(localTime / 28 ms))²`, matching the sampled design: a glyph is already 67.35% visible 12 ms after it starts, while it is still 5.32 pt below rest. A slower reveal hides the initial upward travel and changes the apparent amplitude. The two icon sheets fade linearly within their separate intervals. Native text is painted into one fixed 18 pt line box, with each revealed glyph centered in its current advance column.


### Continuous inserted check stroke

`21.06.3 Continuous check stroke` owns the inserted-result path, with an independent playback flow in `21.11`. Keep the 18 pt canvas centered on the capsule and the existing optical correction. Its centerline is A = (3, 9.75), B = (6.75, 13.5), C = (15, 5.25), with a 1.5 pt opaque white stroke and round caps and join. One tip travels from the upper-left start down to B, then continuously up and right to C. Reach B at 100 ms and C at 320 ms, following cumulative path length at constant speed.

Use one vector path throughout. The portion already drawn stays visible while its tip advances. Do not cross-fade the left and right legs, scale in the second leg, or replace the partial drawing with a different SVG at completion. The preview keeps the complete A–B–C vector unchanged in every frame, including its bounds and transform. A left-anchored clipping frame reveals the stroke up to the advancing tip, with a 1.5 pt round tip at that endpoint. Only the reveal width and tip position animate. Never resize or recenter the vector from its currently visible bounds: every already drawn point must remain at its final coordinate. The saved-file check verifies the full contour, fixed origin, reveal edge and tip against the shared Lucide master. At progress 1, retain the same complete path, origin, stroke and optical offset. Reduce Motion shows that same settled contour immediately.

The native `check_trace` uses these canonical coordinates and cumulative lengths. Its round-ended segment contours form one nonzero-filled union painted once, with no final SVG handoff. Even-odd filling would subtract their shared bend and must not be used. The floating HUD and onboarding use the same renderer.

After the stroke completes at 320 ms, `21.11` adds one flat white halo aligned to that same fixed contour. The halo rises over 180 ms, rests for 80 ms, then fades over 280 ms. Keep the clear completed check visible for another 100 ms before the capsule fades out over 180 ms, finishing at 1140 ms. Use ease-in-out for the light and exit, with no sudden brightness step. The review loop waits 1800 ms before replaying so repeated playback does not resemble flashing. Keep the icon, capsule and halo geometry stationary throughout: only light opacity changes, with no scale pulse, perspective, extrusion or repeated flash. The review peak uses two blurred copies of the contour, 3.5 pt with 4 pt blur at 35% opacity and 2.5 pt with 1.5 pt blur at 55% opacity. The sharp 1.5 pt check remains visible underneath. Reduce Motion skips the halo and animated exit while retaining the completed check feedback. The native renderer caches the two blurred white contour layers per window and animates only their opacity. The controller keeps the window alive through the full feedback and exit.
