//! The window Dito opens instead of exiting when its settings or history cannot be used (design 20.10).
//!
//! `AppState::build` fails before GPUI exists, so this starts GPUI without any app state: no hotkeys, HUD, menu bar
//! or dictation, only the window. Restoring the default settings builds the app in this same process and hands the
//! screen over to its first window, Onboarding.

mod failure;
mod view;

use anyhow::Result;
use dito_core::config::Appearance;
use dito_core::i18n::system_language;
use gpui_kit::QuitMode;

use crate::assets::AppAssets;
use crate::opts::Opts;
use crate::startup;

pub use failure::Failure;
use view::Ending;
pub use view::{Launch, open};

/// Show the window for `error` and run until the person quits or the launch carries on. An error that has no window,
/// and every error of a run nobody watches (smoke test, screenshots), is returned unchanged.
pub fn run(error: anyhow::Error, opts: Opts, rt: tokio::runtime::Handle) -> Result<()> {
    let Some(failure) = Failure::from_error(&error, &opts).filter(|_| !opts.is_headless()) else { return Err(error) };
    tracing::warn!("startup stopped: {error:#}");

    dito_platform::system::configure_text_rendering();
    // The window is all this process has, so closing it ends the process, until a carried-on launch takes over.
    let app = gpui_kit::application().with_assets(AppAssets).with_quit_mode(QuitMode::LastWindowClosed);
    app.on_reopen(startup::reopen);
    let ending = Ending::default();
    let left = ending.0.clone();
    app.run(move |cx| {
        cx.set_global(ending);
        startup::init(cx);
        startup::set_app_menu(system_language(), cx);
        if open(failure, Appearance::System, Launch { opts, rt }, true, cx).is_none() {
            cx.quit();
        }
    });
    // An error the window had no screen for (see `FailureView::carry_on`) ends the process as any startup error does.
    left.take().map_or(Ok(()), Err)
}
