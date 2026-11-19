use std::cell::{Cell, RefCell};
use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::Palette;

pub fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c,
            '\u{1100}'..='\u{11ff}'
                | '\u{3000}'..='\u{9fff}'
                | '\u{ac00}'..='\u{d7af}'
                | '\u{ff00}'..='\u{ffef}'
        )
    })
}

fn is_cyrillic(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{0400}'..='\u{052f}'))
}

static LANGUAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Language-dependent typography reads it: the CJK or script typeface (see [`cjk_family`]).
pub fn set_ui_language(lang: dito_core::i18n::Lang) {
    LANGUAGE.store(
        dito_core::i18n::Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0) as u8,
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub fn ui_language() -> dito_core::i18n::Lang {
    let all = dito_core::i18n::Lang::ALL;
    all[LANGUAGE.load(std::sync::atomic::Ordering::Relaxed) as usize % all.len()]
}

pub fn cjk_family() -> &'static str {
    use dito_core::i18n::Lang;
    match ui_language() {
        Lang::Zh => "IBM Plex Sans SC",
        Lang::ZhHant => "PingFang TC",
        Lang::Ko => "Apple SD Gothic Neo",
        Lang::Mn => "Helvetica Neue",
        Lang::Ja | Lang::En | Lang::Fr => "M PLUS 1p",
    }
}

/// What a text's characters say about its typeface. Text that is drawn every frame keeps this instead of scanning
/// itself again; the family is still looked up per frame, because the CJK one follows the UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Latin,
    Cyrillic,
    Hangul,
    Cjk,
}

impl Script {
    pub fn of(s: &str) -> Self {
        if is_cjk(s) {
            if s.chars().any(|c| matches!(c, '\u{ac00}'..='\u{d7af}' | '\u{1100}'..='\u{11ff}')) {
                return Self::Hangul;
            }
            return Self::Cjk;
        }
        if is_cyrillic(s) {
            return Self::Cyrillic;
        }
        Self::Latin
    }

    pub fn family(self) -> &'static str {
        use dito_core::i18n::Lang;
        match self {
            Self::Hangul => "Apple SD Gothic Neo",
            Self::Cjk => match ui_language() {
                Lang::Zh => "IBM Plex Sans SC",
                Lang::ZhHant => "PingFang TC",
                _ => "M PLUS 1p",
            },
            Self::Cyrillic => "Helvetica Neue",
            Self::Latin => "Outfit",
        }
    }
}

pub fn family_for(s: &str) -> &'static str {
    Script::of(s).family()
}

pub fn text(s: impl Into<SharedString>, size: f32, weight: FontWeight, color: Hsla) -> Div {
    let s: SharedString = s.into();
    div().font_family(family_for(&s)).text_size(px(size)).font_weight(weight).text_color(color).child(s)
}

/// Bytes of text the private text system measures before it is replaced, each call counting `MEASURE_COST` more.
/// GPUI empties a layout cache only when its window finishes a frame, and this one has no window, so it would keep
/// every text, size and wrap width it has ever measured. A layout holds 24 bytes per glyph, which keeps the cache
/// within a few megabytes, while a screen that measures the same labels every frame reshapes them every few seconds.
const MEASURE_BUDGET: usize = 256 * 1024;
const MEASURE_COST: usize = 64;

thread_local! {
    static TEXT_SYSTEM: RefCell<Option<WindowTextSystem>> = const { RefCell::new(None) };
    static MEASURED: Cell<usize> = const { Cell::new(0) };
}

/// Called before a measurement borrows the text system, so no layout from the dropped cache is in use.
fn spend_measure_budget(s: &str) {
    let cost = s.len() + MEASURE_COST;
    let measured = MEASURED.get() + cost;
    if measured <= MEASURE_BUDGET {
        MEASURED.set(measured);
        return;
    }
    MEASURED.set(cost);
    TEXT_SYSTEM.with(|t| {
        if let Some(ts) = t.borrow_mut().as_mut() {
            *ts = WindowTextSystem::new(Arc::clone(ts));
        }
    });
}

pub fn init_text_system(cx: &App) {
    TEXT_SYSTEM.with(|t| *t.borrow_mut() = Some(WindowTextSystem::new(cx.text_system().clone())));
}

/// Measured as the renderer will lay the text out.
pub fn text_width(s: &str, size: f32, weight: FontWeight) -> f32 {
    spend_measure_budget(s);
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

/// How far below the top of a `line_height` box the renderer puts the baseline of `s`. Boxes centered on each other
/// do not line up text in different faces or sizes; offsetting each by the difference of these does.
pub fn baseline_in_line(s: &str, size: f32, weight: FontWeight, line_height: f32) -> f32 {
    TEXT_SYSTEM.with(|t| match t.borrow().as_ref() {
        Some(ts) => {
            let mut f = font(family_for(s));
            f.weight = weight;
            f32::from(ts.baseline_offset(ts.resolve_font(&f), px(size), px(line_height)))
        }
        None => (line_height - size) / 2. + size * 0.8,
    })
}

/// For windows that fit their text, where the height follows the line count.
pub fn text_lines(s: &str, family: impl Into<SharedString>, size: f32, weight: FontWeight, width: f32) -> usize {
    spend_measure_budget(s);
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

/// One line as tall as its type: for labels a control centers itself.
pub fn label(s: &str, size: f32, weight: FontWeight, color: Hsla) -> Div {
    text(s.to_string(), size, weight, color).line_height(relative(1.))
}

/// Only the wordmark is letter-spaced (`design/README.md`, tracking). GPUI has no letter-spacing: every glyph is its
/// own flex item followed by `tracking` em.
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
