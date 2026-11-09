//! Small UI kit matching the FigCraft design: controls, pill buttons, grouped cards.

mod appearance_seg;
mod button;
mod callbacks;
mod format;
mod glass;
mod history_tip;
mod hover;
mod inputs;
mod layout;
mod menu_button;
mod menu_panel;
mod modal;
mod motion;
mod plate;
mod popup_menu;
mod primitives;
mod progress_bar;
mod radio;
mod requests;
mod scope;
mod segmented;
mod select;
mod select_badge;
mod switch;
mod test_state;
mod tip_bubble;
mod tooltip;
mod tween;
mod typography;
mod watch;

pub use appearance_seg::appearance_seg;
pub use button::{
    BtnKind, BtnSize, CONTROL_H, CircleAppearance, CircleSize, bare_icon_button, button, button_with, circle_control,
    close_button, disabled_button, icon_button, icon_button_with,
};
pub use callbacks::{Cb, cbk, cbk_idx, click_cb};
pub use format::{error_line, error_summary, fmt_bytes, fmt_eta, history_style_label};
pub use glass::{prepare as prepare_card_backdrop, wordmark as page_wordmark};
pub use history_tip::history_tip;
#[cfg(feature = "screenshots")]
pub use hover::force_hover;
pub use hover::{
    HOVER_MS, choice_label, hover_fill, hover_fill_if, hover_pop, hover_pop_scaled, hover_tracker, is_hovered,
    is_hovered_in, reset_hover, reset_hover_in,
};
pub use inputs::{
    blur_on_outside, field, focus_on_click, form_field, notify_on_focus, ring_overlay, text_input, with_ring,
};
pub use layout::{StatusTone, card, drag_area, group, page_title, row, status_chip, status_tag, tag};
pub use menu_button::menu_button;
pub use modal::modal_layer;
pub use motion::Motion;
pub use plate::{GLASS_MS, coverage, plate_state};
pub use popup_menu::{MenuItem, MenuTone, popup_menu, popup_menu_grouped};
pub use primitives::{hex_alpha, icon, mix, mix_rgb, transparent, white};
pub use progress_bar::progress_bar;
pub use radio::radio;
pub use requests::Requests;
pub use scope::{MotionScope, ScopeHandle, motion_scope, with_motion_scope};
pub use segmented::{SegContent, SegOption, SegStyle, seg, seg_with};
pub use select::{SelectItem, select, select_disabled, select_items, select_multi, select_trigger};
pub use select_badge::SelectBadge;
pub use switch::switch;
pub use test_state::TestState;
pub use tip_bubble::TIP_BUBBLE_H;
pub use tooltip::{dismiss_tips, tip_above, tip_label, tip_pop, tip_pop_with, tooltip_root};
pub use tween::{spring, tweened, tweened_at};
pub use typography::{
    cjk_family, init_text_system, is_cjk, muted12, set_ui_language, text, text_tracked, text_width, tracked,
    tracking_for,
};
pub use watch::{notify_on_change, observe};

pub(crate) use menu_panel::{float as float_menu, menu_panel, menu_row, menu_separator};
