//! One speaker's live lane inside the bottom panel: who is speaking, the word being heard, and the words already
//! recognized, inside a three-line viewport.

use dito_core::store::Speaker;
use dito_core::transcription::{LiveText, SessionState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TranscriptionPage, speaker_label};
use crate::settings::{Ctx, Edge, edge_fade};
use crate::theme::Palette;
use crate::timefmt::hhmm;
use crate::ui::{self, text};

/// `design:tools/recording` measurements.
const LANE_PAD_X: f32 = 14.;
const LANE_PAD_TOP: f32 = 12.;
const LANE_PAD_BOTTOM: f32 = 15.;
const VIEWPORT_H: f32 = 66.;
const BODY_SIZE: f32 = 14.;
const BODY_LINE: f32 = 22.;
const FADE: f32 = 20.;
const THUMB_W: f32 = 2.5;
const THUMB_H: f32 = 26.;
/// The sweep over unsettled words: its half width in characters and its speed in characters a second.
const SWEEP_BAND: f32 = 6.;
const SWEEP_SPEED: f32 = 18.;

pub(super) struct Geometry {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) text_width: f32,
    pub(super) body_opacity: f32,
    pub(super) show_status: bool,
}

/// `t` is the animation clock in seconds for the sweep; `None` holds it still.
pub(super) fn lane(page: &TranscriptionPage, c: &Ctx, speaker: Speaker, geometry: Geometry, t: Option<f32>) -> Div {
    let Geometry { width, height, text_width, body_opacity, show_status } = geometry;
    let p = &c.p;
    let live = page.status.live.iter().find(|l| l.speaker == speaker);
    let started_ts = match page.status.state {
        SessionState::Recording { started_ts, .. } => Some(started_ts),
        _ => page.open.as_ref().map(|open| open.summary.started_ts),
    };
    let inner = text_width - 2. * LANE_PAD_X - 2.;
    let card = div()
        .w(px(width))
        .h(px(height))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(8.))
        .pt(px(LANE_PAD_TOP.min((height - 20.).max(0.) / 2.)))
        .pb(px(LANE_PAD_BOTTOM))
        .px(px(LANE_PAD_X))
        .rounded(px(12.))
        .bg(p.card)
        .border_1()
        .border_color(p.border)
        .when(height >= 26., |d| {
            d.child(lane_head(p, c, speaker, live, started_ts, show_status, listening_status(page)))
        });
    if body_opacity <= 0. {
        return card;
    }
    let (words, overflow) = match live {
        Some(live) => {
            (live_words(p, &live.settled, &live.unsettled, inner, t), overflow(&live.settled, &live.unsettled, inner))
        }
        None => (live_words(p, "", "", inner, t), false),
    };
    // Words start right under the speaker while they fit; once they pass three lines the newest one stays in view.
    let words = div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .when(overflow, |column| column.justify_end())
        .overflow_hidden()
        .child(words);
    let viewport = div()
        .relative()
        .w_full()
        .h(px(VIEWPORT_H))
        .overflow_hidden()
        .child(words)
        .when(overflow, |v| v.child(edge_fade(Edge::Top, p.card, FADE, THUMB_W + 4.)))
        .when(overflow, |v| {
            v.child(
                div()
                    .absolute()
                    .top(px(20.))
                    .right(px(2.))
                    .w(px(THUMB_W))
                    .h(px(THUMB_H))
                    .rounded(px(99.))
                    .bg(p.border_strong),
            )
        });
    card.child(viewport.opacity(body_opacity))
}

fn lane_head(
    p: &Palette,
    c: &Ctx,
    speaker: Speaker,
    live: Option<&LiveText>,
    started_ts: Option<i64>,
    full: bool,
    status: (dito_core::i18n::Tr, f32),
) -> Div {
    let (dot, ink) = if speaker == Speaker::Me { (p.primary, p.accent_text) } else { (p.muted, p.text) };
    let off = dito_platform::local_utc_offset_secs();
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .child(div().size(px(6.)).flex_none().rounded_full().bg(dot))
                .child(
                    text(c.t(speaker_label(speaker)), 12., FontWeight::SEMIBOLD, ink)
                        .line_height(px(18.))
                        .whitespace_nowrap(),
                )
                .children(live.filter(|_| full).map(|live| {
                    let at = started_ts.map_or_else(
                        || crate::timefmt::clock(live.start_ms),
                        |started_ts| hhmm(started_ts + live.start_ms as i64 / 1000, off),
                    );
                    text(at, 11., FontWeight::NORMAL, p.muted).line_height(px(18.))
                })),
        )
        .when(full, |head| {
            head.child(
                text(c.t(status.0), 11., FontWeight::MEDIUM, p.muted)
                    .line_height(px(18.))
                    .whitespace_nowrap()
                    .opacity(status.1),
            )
        })
}

/// The lane's status: `TranscriptionListening` breathing between 100 and 80 percent while recording, `TranscriptionPaused` held steady while
/// paused (`design:tools/recording`). Reduce Motion holds the label steady too.
fn listening_status(page: &TranscriptionPage) -> (dito_core::i18n::Tr, f32) {
    if page.is_paused() {
        return (dito_core::i18n::Tr::TranscriptionPaused, 1.);
    }
    let opacity = if ui::Motion::system().is_reduced() { 1. } else { pulse(page.born.elapsed().as_secs_f32()) };
    (dito_core::i18n::Tr::TranscriptionListening, opacity)
}

/// Each leg eases over 650 ms and each end holds for 150 ms.
fn pulse(t: f32) -> f32 {
    const LEG: f32 = 650.;
    const HOLD: f32 = 150.;
    const DIP: f32 = 0.2;
    let ms = (t * 1000.) % (2. * (LEG + HOLD));
    if ms < LEG {
        1. - DIP * ui::ease(ms / LEG)
    } else if ms < LEG + HOLD {
        1. - DIP
    } else if ms < 2. * LEG + HOLD {
        1. - DIP + DIP * ui::ease((ms - LEG - HOLD) / LEG)
    } else {
        1.
    }
}

fn overflow(settled: &str, unsettled: &str, width: f32) -> bool {
    let lines =
        ui::text_lines(&format!("{settled}{unsettled}"), ui::cjk_family(), BODY_SIZE, FontWeight::NORMAL, width - 8.);
    lines > (VIEWPORT_H / BODY_LINE) as usize
}

/// Settled words, then the words still being heard as a sweeping gradient that ends in an ellipsis. Trailing
/// punctuation of the unsettled words goes before the ellipsis.
fn live_words(p: &Palette, settled: &str, unsettled: &str, width: f32, t: Option<f32>) -> Div {
    let mut s = settled.to_string();
    let mut highlights = Vec::new();
    let unsettled = unsettled.trim_end_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, '。' | '、' | '．' | '，' | '.' | ',' | '！' | '？' | '!' | '?' | '…')
    });
    if !unsettled.trim().is_empty() {
        let from = s.len();
        s.push_str(unsettled);
        s.push('…');
        let faint = ui::mix_rgb(p.muted, p.card, 0.55);
        let count = s[from..].chars().count() as f32;
        // The crest runs past both ends so the sweep fades in and out.
        let crest = t.map_or(-SWEEP_BAND, |t| (t * SWEEP_SPEED) % (count + 2. * SWEEP_BAND) - SWEEP_BAND);
        for (n, (i, ch)) in s[from..].char_indices().enumerate() {
            let lift = (1. - (n as f32 - crest).abs() / SWEEP_BAND).max(0.);
            let color = ui::mix_rgb(faint, p.muted, lift);
            highlights.push((
                from + i..from + i + ch.len_utf8(),
                HighlightStyle { color: Some(color), ..Default::default() },
            ));
        }
    }
    div()
        .w(px(width - 8.))
        .flex_none()
        .font_family(ui::cjk_family())
        .text_size(px(BODY_SIZE))
        .line_height(px(BODY_LINE))
        .text_color(p.text)
        .child(StyledText::new(s).with_highlights(highlights))
}
