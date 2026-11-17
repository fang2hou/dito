//! The first-run intro film. It plays over the welcome page, which is already in place under it, and its last frame
//! is that page, so the hand-over leaves nothing to swap.

use std::path::PathBuf;
use std::time::Instant;

use dito_core::config::Appearance;
use dito_core::i18n::{Lang, Tr};
use dito_platform::IntroVideo;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::Ctx;
use crate::state::{AppState, data_dir};
use crate::theme;
use crate::ui::{self, BtnKind, BtnSize, cbk};

use super::Onboarding;

const SEEN_FILE: &str = "intro-seen";
/// Held Ctrl plays the film this much faster, so repeated test runs stay short.
const FAST_RATE: f32 = 3.;
const SKIP_FADE_S: f32 = 0.2;
/// The skip button appears after the film has settled in, and fades in over the next 0.4 s.
const SKIP_DELAY_S: f64 = 1.2;
/// The button leaves this long before the end: the film closes on the welcome page, which has no skip button.
const SKIP_LEAVES_BEFORE_END_S: f64 = 8.;
/// The film opens on the wizard's own dark window colour, so the first frame can be this before it is decoded.
const FILM_BG: u32 = 0x444139;

pub(super) struct Intro {
    video: IntroVideo,
    focus: FocusHandle,
    frame: Option<SurfaceSource>,
    ending: Option<Instant>,
}

/// Whether the film plays. A film that was seen never plays again, except when `DITO_INTRO=always`; mock runs keep
/// no record, so every one of them plays it.
fn should_play(seen: bool, headless: bool, reduced_motion: bool, env: Option<&str>) -> bool {
    match env {
        Some("never") => false,
        _ if headless => false,
        Some("always") => true,
        _ => !seen && !reduced_motion,
    }
}

/// The film of the UI language: Japanese, Simplified Chinese (also for Traditional Chinese), Korean, and English for
/// every other language.
fn film_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "dito-intro-ja.mp4",
        Lang::Zh | Lang::ZhHant => "dito-intro-zh.mp4",
        Lang::Ko => "dito-intro-ko.mp4",
        Lang::En | Lang::Mn | Lang::Fr => "dito-intro-en.mp4",
    }
}

fn film_path(lang: Lang) -> Option<PathBuf> {
    let dir = std::env::var_os("DITO_INTRO_DIR")
        .map(PathBuf::from)
        .or_else(|| dito_platform::system::bundle_resources().map(|r| r.join("intro")))?;
    let path = dir.join(film_name(lang));
    path.is_file().then_some(path)
}

fn seen_marker(st: &AppState) -> PathBuf {
    data_dir(&st.opts).join(SEEN_FILE)
}

impl Intro {
    pub(super) fn start(st: &AppState, cx: &mut Context<Onboarding>) -> Option<Self> {
        let seen = !st.opts.mock && seen_marker(st).exists();
        let env = std::env::var("DITO_INTRO").ok();
        let reduced = ui::Motion::system().is_reduced();
        if !should_play(seen, st.opts.is_headless(), reduced, env.as_deref()) {
            return None;
        }
        let video = IntroVideo::open(&film_path(st.cfg.get().general.lang())?)?;
        Some(Self { video, focus: cx.focus_handle(), frame: None, ending: None })
    }
}

impl Onboarding {
    /// Called by the window when it opens; the film is dropped as soon as it ends or is skipped.
    pub(super) fn begin_intro(&mut self, cx: &mut Context<Self>) {
        self.intro = Intro::start(&self.st, cx);
        self.intro_played = self.intro.is_some();
    }

    pub(super) fn skip_intro(&mut self, cx: &mut Context<Self>) {
        if let Some(intro) = self.intro.as_mut().filter(|i| i.ending.is_none()) {
            intro.ending = Some(Instant::now());
            cx.notify();
        }
    }

    fn finish_intro(&mut self) {
        self.intro = None;
        if !self.st.opts.mock
            && let Err(e) = std::fs::write(seen_marker(&self.st), b"")
        {
            tracing::warn!("intro marker: {e}");
        }
    }

    /// The palette of the welcome page. It follows the film, which is dark, until the first step slides in.
    pub(super) fn welcome_ctx(&self, c: &Ctx, cx: &App) -> Ctx {
        let p = if self.intro_played && !c.p.dark { theme::palette(Appearance::Dark, cx) } else { c.p };
        Ctx { p, lang: c.lang }
    }

    /// The film's layer, or `None` once it is over. It advances the film one frame per call, so the window keeps
    /// drawing for as long as it plays.
    pub(super) fn intro_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let intro = self.intro.as_mut()?;
        intro.video.set_rate(if window.modifiers().control { FAST_RATE } else { 1. });
        if let Some(frame) = intro.video.next_frame() {
            intro.frame = Some(frame.into());
        }
        let fade = intro.ending.map(|at| (at.elapsed().as_secs_f32() / SKIP_FADE_S).min(1.));
        if intro.video.finished() || fade == Some(1.) {
            self.finish_intro();
            return None;
        }
        if let Some(f) = fade {
            intro.video.set_volume(1. - f);
        }
        window.request_animation_frame();
        let position = intro.video.position_secs();
        let skip_in = (position - SKIP_DELAY_S) / 0.4;
        let skip_out = (intro.video.duration_secs() - SKIP_LEAVES_BEFORE_END_S - position) / 0.4;
        let skip_alpha = skip_in.min(skip_out).clamp(0., 1.) as f32;
        let frame = intro.frame.clone();

        let focus = intro.focus.clone();
        if !focus.is_focused(window) {
            focus.focus(window, cx);
        }
        let c = Ctx::new(&self.st, cx);
        let dark = Ctx { p: theme::palette(Appearance::Dark, cx), lang: c.lang };
        let skip = ui::button_with(
            "intro-skip",
            &dark.p,
            &dark.t(Tr::OnboardingIntroSkip),
            BtnKind::Secondary,
            BtnSize::Regular,
            None,
            None,
            cbk(cx, |this: &mut Onboarding, _, cx| this.skip_intro(cx)),
        );
        Some(
            div()
                .id("intro-film")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(rgb(FILM_BG))
                .occlude()
                .track_focus(&focus)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.skip_intro(cx);
                    }
                }))
                .when_some(frame, |d, frame| d.child(surface(frame).size_full().object_fit(ObjectFit::Fill)))
                .child(div().absolute().right(px(24.)).bottom(px(24.)).opacity(skip_alpha).child(skip)),
        )
    }
}

#[cfg(test)]
mod tests {
    // Named imports: the glob of the parent module would bring in the UI framework's own `test` attribute.
    use dito_core::i18n::Lang;

    use super::{film_name, should_play};

    #[test]
    fn the_film_plays_once_unless_forced() {
        assert!(should_play(false, false, false, None));
        assert!(!should_play(true, false, false, None));
        assert!(should_play(true, false, false, Some("always")));
    }

    #[test]
    fn automated_runs_and_reduced_motion_never_play_it() {
        assert!(!should_play(false, true, false, None));
        assert!(!should_play(false, true, false, Some("always")));
        assert!(!should_play(false, false, true, None));
        assert!(!should_play(false, false, false, Some("never")));
    }

    #[test]
    fn every_language_has_a_film() {
        assert_eq!(film_name(Lang::Ja), "dito-intro-ja.mp4");
        assert_eq!(film_name(Lang::Zh), "dito-intro-zh.mp4");
        assert_eq!(film_name(Lang::ZhHant), "dito-intro-zh.mp4");
        assert_eq!(film_name(Lang::Ko), "dito-intro-ko.mp4");
        for lang in [Lang::En, Lang::Mn, Lang::Fr] {
            assert_eq!(film_name(lang), "dito-intro-en.mp4");
        }
    }
}
