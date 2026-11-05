//! Small UI kit matching the FigCraft design: 36px controls, pill buttons, grouped cards.

mod callbacks;
mod controls;
mod format;
mod glass;
mod inputs;
mod layout;
mod menus;
mod motion;
mod primitives;
mod tooltip;
mod typography;

// Kit-internal helpers are not re-exported; sibling modules reach them via `super::<module>`: the
// callback aliases `IndexCb`/`ToggleCb`, the `TextInput` element behind `text_input`, the shared
// field shell `field_chrome`, `solid_tag` (used by `status_tag`), the motion internals `Registry`
// and `PlateState` (pages go through `MotionScope`/`plate_state`), the tooltip wiring
// `tip_above`/`tip_label` (used by the controls), and the typography internals `family_for` and
// `ui_language`.

pub use callbacks::{Cb, cbk, cbk_idx, click_cb};
pub use controls::{
    BtnKind, BtnSize, CONTROL_H, SegStyle, TestState, appearance_seg, bare_icon_button, button, button_with,
    choice_label, close_button, drag_area, hover_fill, hover_fill_if, icon_button, progress_bar, radio, seg, seg_with,
    switch,
};
pub use format::{error_line, error_summary, fmt_bytes, fmt_eta, style_label};
pub use glass::{prepare as prepare_card_backdrop, wordmark as page_wordmark};
pub use inputs::{field, form_field, notify_on_focus, ring_overlay, text_input, with_ring};
pub use layout::{card, group, page_title, row, status_chip, status_tag, tag};
pub use menus::{
    MenuItem, MenuTone, SelectBadge, SelectItem, menu_button, popup_menu, select, select_disabled, select_items,
};
#[cfg(feature = "screenshots")]
pub use motion::force_hover;
pub use motion::{
    GLASS_MS, HOVER_MS, MotionScope, ScopeHandle, coverage, hover_pop, hover_pop_scaled, hover_tracker, is_hovered,
    is_hovered_in, motion_scope, plate_state, reset_hover, spring, tweened, tweened_at, with_motion_scope,
};
pub use primitives::{hex_alpha, hud_icon, icon, mix, transparent, white};
pub use tooltip::{TIP_BUBBLE_H, history_tip, tip_pop, tip_pop_with};
pub use typography::{
    cjk_family, init_text_system, is_cjk, muted12, set_ui_language, text, text_tracked, text_width, tracked,
    tracking_for,
};
