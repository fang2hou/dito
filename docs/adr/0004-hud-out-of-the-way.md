# ADR-0004: The HUD never takes focus and never gets in the way

- **Status**: accepted
- **Date**: 2026-11-05

## Context

The HUD floats over the app the user is typing into. If it takes focus, text goes to Dito instead of the target. If it covers menus or blocks clicks, it breaks the user's work. Earlier versions used a normal window: clicking ✓ or Stop made the HUD key, and dictations silently landed on the clipboard.

## Decision

The HUD is a non-activating panel that can't become key on click, sits above app windows but below menus and tooltips, lets clicks pass through everywhere except its controls, and closes when the dictation ends. Hover and click-through come from polling the pointer, because the panel receives no mouse events while it lets clicks through. Its main surfaces are opaque with one soft shadow. Cancel and Finish use the scoped material approved in `design:hud/buttons`. GPUI has no native backdrop filter, so the renderer uses the approved 96% resting tint and opaque hover fallback, with a local spread of the procedural ocean underneath; foreground icons remain opaque. Do not replace the panel with a system-wide blur backdrop.

Amended 2026-11-11 for command mode (`design:hud/command`): the panel may become the key window while a command window with a text field or editable text is open (the instruction field, the text window, the answer and confirm windows). The capsule, the choice window and every dictation state still never take focus.

Amended again the same day, after Japanese and Chinese input methods failed in those fields. Key status alone is not enough: while another app is in front, the panel receives the key events, but the system offers them to that app's text client, so a Japanese or Chinese source shows only its badge and types plain Latin (no marked text, no candidate window, no switcher). Input methods compose only for the app in front. So while a command window with a text field is open, Dito takes the front: it remembers the app that had it, makes the panel key and activates Dito. Only the key window comes forward, so a Settings window behind stays behind, and an accessory app has no menu bar to swap in. Activation uses `activateWithOptions` with the ignore-other-apps option, because the cooperative form is refused for an app that is not in front.

Amended a third time the same day, after a pinyin composition showed its candidate bar behind the panel. The HUD normally sits at the status window level (25). An input method's candidate and composition windows belong to the client's process and sit at the Dock level (20), measured with `CGWindowListCopyWindowInfo` while composing with the Japanese and Shuangpin sources, so the status level covered them. While a command window with a text field is open, the panel drops to the modal panel level (8): still above app windows and their floating panels, below the candidates. It returns to the status level in the same key change that gives the front back. The capsule, the choice window and every dictation state keep the status level.

When the command window closes, or goes on to a state without a text field, the panel resigns key and the remembered app gets the front and its key window back. A command that writes into that app (replacing the selection, inserting at the cursor) waits for it, 400 ms at most; `Selection::replace` still waits for the source app's keyboard focus as before. If the user has already moved to another app, nothing is handed back. While a field holds composing text, Esc belongs to the input method: the hotkey tap neither reports nor swallows it, so the composition is cancelled and the window stays; without composing text Esc closes the window as before.

During the trial in `design:onboarding`, the real dictation HUD may temporarily anchor inside the focused Try It editor, centered 16 pt above its bottom edge. The editor reserves space so text and selection stay above the capsule. Its view owns the measured bounds, and the HUD borrows them only while the wizard is active and that editor is focused. Moving the wizard moves the anchor; leaving the step, hiding the wizard or focusing another target restores the configured placement without changing preferences. This remains a non-activating HUD, and recording, processing and insertion feedback share the temporary position.

## Alternatives Considered

### Normal window

- Pros: Simple
- Cons: Clicks make it key and text lands on the clipboard
- Why not chosen: Rejected after failures

### Floating window

- Pros: Stays above
- Cons: No hover while unfocused, ties with other apps' panels
- Why not chosen: Rejected

### System blur backdrop

- Pros: Native look
- Cons: Tint and radius can't be controlled
- Why not chosen: Rejected

## Consequences

Every new HUD control must work through the polled hover and click-through. Apply AppKit window moves after releasing the GPUI app borrow because the move callback runs synchronously. The HUD advances on its display link and opts out of inactive-window throttling. Quiet frames can skip drawing. Dictation features must not require focus. A command window may hold a text field only through the key-status and front switch above; while it is key, nothing may be typed for the user, because the keystrokes would land in Dito. Whatever ends such a window, including the window being removed, must give the front back. The multi-line field already implements GPUI's input handler (marked range, `replace_and_mark_text_in_range`, `bounds_for_range`) like the single-line input, so no field code is involved. The editors read their colors from the app-wide theme, which every window sets from its own palette while it renders (a Settings window in the dark appearance next to a HUD in the light one), and a `Root` takes the text color from the theme before its view renders. So the HUD's root element sets its own text color, which keeps the editors' text from alternating between the two palettes.

## Pitfalls

Moves that look reasonable but go against this decision:

- Switching to a Normal or Floating window, or removing `configure_nonactivating_panel`.
- Replacing the pointer poll with hover events.
- Raising the HUD above menus (PopUp's default level), or keeping the status level while a text field is open, which hides the input method's candidate window.
- Adding controls that need keyboard focus to the capsule, the choice window or any dictation state.
- Activating Dito for anything but a command window with a text field, or leaving Dito in front or the panel key after that window closes.
- Swallowing Esc in the hotkey tap while a field holds composing text, which keeps the input method from cancelling its composition.

## Review Triggers

GPUI gains a native non-activating panel or window levels; macOS changes panel focus rules; macOS stops honoring forced activation, or input methods start composing for a key panel of an app that is not in front.

## Code

`crates/dito-app/src/hud/window.rs`, `crates/dito-platform/src/system/hud_window.rs` (`HudNativeWindow::keep_panel_inactive`, `pointer`, `set_click_through`, `set_accepts_key` with `HudKeyChange` (also the level switch, `TEXT_WINDOW_LEVEL`), `give_front_back`, `source_app_in_front`), `crates/dito-platform/src/hotkey/machine.rs` (`escape_to_ime`).
