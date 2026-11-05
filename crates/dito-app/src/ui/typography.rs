use std::cell::RefCell;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::Palette;

pub fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{3000}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}'))
}

static LANGUAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Tell the UI which language it is shown in. Language-dependent typography reads it: the CJK typeface
/// (Plex SC for Chinese, M PLUS 1p for Japanese) and letter-spacing (see [`tracking_for`]).
pub fn set_ui_language(lang: dito_core::i18n::Lang) {
    LANGUAGE.store(
        dito_core::i18n::Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0) as u8,
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub fn ui_language() -> dito_core::i18n::Lang {
    dito_core::i18n::Lang::ALL[LANGUAGE.load(std::sync::atomic::Ordering::Relaxed) as usize % 3]
}

/// Font for CJK text (and the UI root) in the current display language.
pub fn cjk_family() -> &'static str {
    if ui_language() == dito_core::i18n::Lang::Zh { "IBM Plex Sans SC" } else { "M PLUS 1p" }
}

/// Letter-spacing (in em) for `s` where the design asks for `cjk_em`. Wide tracking is a Japanese/Chinese reading
/// aid; Latin text keeps its natural spacing, so it is zero for English and for any string without CJK glyphs.
/// Put every language-specific typographic difference here, so screens never branch on the language themselves.
pub fn tracking_for(s: &str, cjk_em: f32) -> f32 {
    if ui_language() != dito_core::i18n::Lang::En && is_cjk(s) { cjk_em } else { 0. }
}

pub fn family_for(s: &str) -> &'static str {
    if is_cjk(s) { cjk_family() } else { "Outfit" }
}

/// Plain text with the design's font choice (IBM Plex Sans SC for Chinese, M PLUS 1p for Japanese, Outfit for Latin).
pub fn text(s: impl Into<SharedString>, size: f32, weight: FontWeight, color: Hsla) -> Div {
    let s: SharedString = s.into();
    div().font_family(family_for(&s)).text_size(px(size)).font_weight(weight).text_color(color).child(s)
}

thread_local! {
    static TEXT_SYSTEM: RefCell<Option<WindowTextSystem>> = const { RefCell::new(None) };
}

/// Give the UI kit the text system so controls can size themselves to their labels.
pub fn init_text_system(cx: &App) {
    TEXT_SYSTEM.with(|t| *t.borrow_mut() = Some(WindowTextSystem::new(cx.text_system().clone())));
}

/// Width of one line of UI text, as the renderer will lay it out.
pub fn text_width(s: &str, size: f32, weight: FontWeight) -> f32 {
    TEXT_SYSTEM.with(|t| match t.borrow().as_ref() {
        Some(ts) => {
            let mut f = font(family_for(s));
            f.weight = weight;
            let run = TextRun {
                len: s.len(),
                font: f,
                color: hsla(0., 0., 0., 1.),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f32::from(ts.shape_line(SharedString::from(s.to_string()), px(size), &[run], None).width)
        }
        None => s.chars().count() as f32 * size,
    })
}

/// Text with the design's CJK letter-spacing `cjk_em`, applied only where [`tracking_for`] says so.
pub fn text_tracked(s: &str, size: f32, weight: FontWeight, color: Hsla, cjk_em: f32) -> Div {
    let em = tracking_for(s, cjk_em);
    if em == 0. {
        return text(s.to_string(), size, weight, color).line_height(relative(1.));
    }
    let mut d = div().flex().items_center().gap(px(size * em));
    for ch in s.chars() {
        d = d.child(text(ch.to_string(), size, weight, color).line_height(relative(1.)));
    }
    d
}

/// Text with letter-spacing (GPUI has none): every glyph is its own flex item followed by `tracking` em.
pub fn tracked(s: &str, size: f32, weight: FontWeight, color: Hsla, tracking: f32) -> Div {
    let mut row = div().flex().items_end();
    for ch in s.chars() {
        // The last glyph is spaced too, like CSS/Figma letter-spacing: whatever follows keeps the same rhythm.
        row = row.child(text(ch.to_string(), size, weight, color).line_height(relative(1.1)).mr(px(size * tracking)));
    }
    row
}

pub fn muted12(p: &Palette, s: impl Into<SharedString>) -> Div {
    text(s, 12., FontWeight::NORMAL, p.muted)
}
