mod assets;
mod dev;
mod hotkeys;
mod hud;
mod mac_focus_warmer;
mod menubar;
mod onboarding;
mod opts;
mod services;
mod settings;
mod startup;
mod state;
mod theme;
mod timefmt;
mod ui;

use anyhow::Result;
use gpui_kit::WindowingRequest;

use crate::assets::AppAssets;
use crate::opts::Opts;
use crate::state::AppState;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("dito=info".parse()?)
                .add_directive("dito_audio=debug".parse()?),
        )
        .init();
    let opts = Opts::parse(std::env::args().skip(1))?;
    dito_core::i18n::set_system_language(opts.system_lang.unwrap_or_else(dito_platform::system::system_language));
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("dito-rt").build()?;
    let st = AppState::build(opts, &rt)?;

    let mut app = gpui_kit::application().with_assets(AppAssets);
    if st.opts.is_headless() {
        // Accessory activation policy: no Dock icon, no menu bar, never the frontmost app.
        app = app.with_windowing(WindowingRequest::Headless);
    }
    app.run(move |cx| startup::run(st, cx));
    Ok(())
}
