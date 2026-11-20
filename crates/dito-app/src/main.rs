mod app_shell;
mod assets;
mod dev;
mod dock;
mod hotkeys;
mod hud;
mod logbook;
mod menubar;
mod onboarding;
mod opts;
mod services;
mod settings;
mod startup;
mod startup_failure;
mod state;
mod theme;
mod timefmt;
mod tools;
mod ui;

use anyhow::Result;
use gpui_kit::WindowingRequest;

use crate::assets::AppAssets;
use crate::opts::Opts;
use crate::state::AppState;

fn main() -> Result<()> {
    logbook::install()?;
    let opts = Opts::parse(std::env::args().skip(1))?;
    dito_core::i18n::set_system_language(opts.system_lang.unwrap_or_else(dito_platform::system::system_language));
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("dito-rt").build()?;
    let st = match AppState::build(opts.clone(), rt.handle()) {
        Ok(st) => st,
        Err(error) => return startup_failure::run(error, opts, rt.handle().clone()),
    };

    dito_platform::system::configure_text_rendering();
    let mut app = gpui_kit::application().with_assets(AppAssets);
    if dock::starts_hidden(&st) {
        // Accessory activation policy: no Dock icon, no menu bar, never the frontmost app.
        app = app.with_windowing(WindowingRequest::Headless);
    }
    app.on_reopen(startup::reopen);
    app.run(move |cx| startup::run(st, cx));
    Ok(())
}
