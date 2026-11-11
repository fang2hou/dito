use std::cell::RefCell;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::Palette;

pub fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{3000}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}'))
}

static LANGUAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Language-dependent typography reads it: the CJK typeface and letter-spacing (see [`tracking_for`]).
pub fn set_ui_language(lang: dito_core::i18n::Lang) {
    LANGUAGE.store(
        dito_core::i18n::Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0) as u8,
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub fn ui_language() -> dito_core::i18n::Lang {
    dito_core::i18n::Lang::ALL[LANGUAGE.load(std::sync::atomic::Ordering::Relaxed) as usize % 3]
}

pub fn cjk_family() -> &'static str {
    if ui_language() == dito_core::i18n::Lang::Zh { "IBM Plex Sans SC" } else { "M PLUS 1p" }
}

/// In em. Wide tracking is a Japanese/Chinese reading aid; Latin text keeps its natural spacing, so it is zero for
/// English and for any string without CJK glyphs. Put every language-specific typographic difference here, so screens
/// never branch on the language themselves.
pub fn tracking_for(s: &str, cjk_em: f32) -> f32 {
    if ui_language() != dito_core::i18n::Lang::En && is_cjk(s) { cjk_em } else { 0. }
}

pub fn family_for(s: &str) -> &'static str {
    if is_cjk(s) { cjk_family() } else { "Outfit" }
}

pub fn text(s: impl Into<SharedString>, size: f32, weight: FontWeight, color: Hsla) -> Div {
    let s: SharedString = s.into();
    div().font_family(family_for(&s)).text_size(px(size)).font_weight(weight).text_color(color).child(s)
}

thread_local! {
    static TEXT_SYSTEM: RefCell<Option<WindowTextSystem>> = const { RefCell::new(None) };
}

pub fn init_text_system(cx: &App) {
    TEXT_SYSTEM.with(|t| *t.borrow_mut() = Some(WindowTextSystem::new(cx.text_system().clone())));
}

/// Measured as the renderer will lay the text out.
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

/// For windows that fit their text, where the height follows the line count.
pub fn text_lines(s: &str, family: impl Into<SharedString>, size: f32, weight: FontWeight, width: f32) -> usize {
    TEXT_SYSTEM.with(|t| match t.borrow().as_ref() {
        Some(ts) => {
            let mut f = font(family);
            f.weight = weight;
            let run = TextRun {
                len: s.len(),
                font: f,
                color: hsla(0., 0., 0., 1.),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            ts.shape_text(SharedString::from(s.to_string()), px(size), &[run], Some(px(width)), None)
                .map_or(1, |lines| lines.iter().map(|line| line.wrap_boundaries.len() + 1).sum())
        }
        None => 1,
    })
}

pub fn text_tracked_width(s: &str, size: f32, weight: FontWeight, cjk_em: f32) -> f32 {
    let em = tracking_for(s, cjk_em);
    if em == 0. {
        return text_width(s, size, weight);
    }
    s.chars().map(|ch| text_width(&ch.to_string(), size, weight)).sum::<f32>()
        + s.chars().count().saturating_sub(1) as f32 * size * em
}

pub fn text_tracked(s: &str, size: f32, weight: FontWeight, color: Hsla, cjk_em: f32) -> Div {
    let em = tracking_for(s, cjk_em);
    if em == 0. {
        return text(s.to_string(), size, weight, color).line_height(relative(1.));
    }
    let mut d = div().flex_none().flex().items_baseline().gap(px(size * em));
    for ch in s.chars() {
        d = d.child(text(ch.to_string(), size, weight, color).line_height(relative(1.)).flex_none());
    }
    d
}

/// GPUI has no letter-spacing: every glyph is its own flex item followed by `tracking` em.
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

#[cfg(test)]
mod tests {
    use dito_core::i18n::Lang;

    use super::{set_ui_language, tracking_for};

    #[test]
    fn english_text_keeps_natural_letter_spacing() {
        set_ui_language(Lang::En);
        assert_eq!(tracking_for("Transcribing", 0.08), 0.);
        assert_eq!(tracking_for("認識中", 0.08), 0.);
        set_ui_language(Lang::Ja);
        assert_eq!(tracking_for("認識中", 0.08), 0.08);
        assert_eq!(tracking_for("Slack", 0.08), 0., "no CJK glyphs, no tracking");
        set_ui_language(Lang::Zh);
        assert_eq!(tracking_for("認識中", 0.08), 0.08);
    }
}
