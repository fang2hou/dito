//! The app's UI kit, built from the FigCraft design.

mod appearance_seg;
mod banner;
mod button;
mod callbacks;
mod config_tiles;
mod copy_button;
mod dialog;
mod form_footer;
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
mod params;
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
mod selectable;
mod switch;
mod test_state;
mod tip_bubble;
mod tooltip;
mod tween;
mod typography;
mod watch;

pub use appearance_seg::appearance_seg;
pub use banner::{BannerAction, banner, dismissible_banner};
pub use button::{
    BtnKind, BtnSize, CONTROL_H, CircleAppearance, CircleSize, bare_icon_button, button, button_with, circle_control,
    close_button, disabled_button, disabled_icon_button, disabled_icon_button_with, icon_button, icon_button_with,
};
pub use callbacks::{Cb, cbk, cbk_copy, cbk_idx, click_cb};
pub use config_tiles::{ConfigTile, TilePlus, TileRowState, cbk_id, cbk_move, tile_row};
pub use copy_button::{copy_button, copy_button_with};
pub use dialog::{ConfirmStyle, DialogAnswer, confirm_dialog};
pub use form_footer::{FooterTrash, TestButton, form_footer};
pub use format::{error_line, error_summary, fmt_bytes, fmt_eta, history_target_label};
pub use glass::{prepare as prepare_card_backdrop, wordmark as page_wordmark};
pub use history_tip::history_tip;
#[cfg(feature = "screenshots")]
pub use hover::force_hover;
pub use hover::{
    HOVER_MS, choice_label, hover_fill, hover_fill_if, hover_pop, hover_pop_scaled, hover_tracker, is_hovered,
    is_hovered_in, reset_hover, reset_hover_in,
};
pub use inputs::{
    TextAreaStyle, blur_on_outside, field, focus_on_click, form_field, new_input, notify_on_focus, ring_overlay,
    text_area, text_input, with_ring,
};
pub use layout::{StatusTone, card, drag_area, group, page_title, row, row_described, status_chip, status_tag, tag};
pub use menu_button::menu_button;
pub use modal::modal_layer;
pub use motion::Motion;
pub use params::{ParamField, parameter_block};
pub use plate::{GLASS_MS, coverage, plate_state};
pub use popup_menu::{AddEntry, MenuItem, MenuTone, add_menu, popup_menu, popup_menu_grouped};
pub use primitives::{hex_alpha, icon, mix, mix_rgb, transparent, white};
pub use progress_bar::progress_bar;
pub use radio::radio;
pub use requests::Requests;
pub use scope::{MotionScope, ScopeHandle, motion_scope, with_motion_scope};
pub use segmented::{SegContent, SegOption, SegStyle, seg, seg_with};
pub use select::{SelectItem, select, select_form, select_form_disabled, select_items, select_multi, select_trigger};
pub use select_badge::SelectBadge;
pub use selectable::{selectable_spans, selectable_text};
pub use switch::switch;
pub use test_state::{ErrorPlacement, TestState, test_summary};
pub use tip_bubble::TIP_BUBBLE_H;
pub use tooltip::{dismiss_tips, tip_above, tip_label, tip_pop, tip_pop_with, tooltip_root};
pub use tween::{fade_value, spring, tweened, tweened_at};
pub use typography::{
    cjk_family, family_for, init_text_system, is_cjk, muted12, set_ui_language, text, text_lines, text_tracked,
    text_width, tracked, tracking_for,
};
pub use watch::{notify_on_change, observe};

// gpui-kit component types the app uses; other modules import them from `ui`, not from gpui-kit.
pub use gpui_kit::component::input::{Editor, EditorState, InputEvent, InputState, Textarea, TextareaState};
pub use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode};
pub use gpui_kit::component::{Root, Theme};

pub(crate) use menu_panel::{float as float_menu, menu_panel, menu_row, menu_separator};
