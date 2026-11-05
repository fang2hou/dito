mod assets;
mod fixtures;
mod hotkeys;
mod hud;
mod hud_style_preview;
mod menubar;
mod onboarding;
mod onboarding_demo;
mod opts;
mod services;
mod settings;
#[cfg(feature = "screenshots")]
mod shots;
mod state;
mod theme;
mod timefmt;
mod ui;

use std::sync::Arc;

use anyhow::Result;
use dito_platform::{HotkeyService, Shortcut};
use gpui_kit::*;
use state::{AppGlobal, AppState, Opts};

actions!(dito, [Quit]);

fn start_hotkeys(st: &Arc<AppState>) {
    let id = st.cfg.get().general.shortcut.clone();
    let shortcut = Shortcut::parse(&id)
        .unwrap_or_else(|| Shortcut::parse(dito_core::config::DEFAULT_SHORTCUT).expect("default shortcut"));
    match HotkeyService::start(shortcut) {
        Ok((service, rx)) => {
            tracing::info!("shortcut tap live ({id})");
            st.hotkeys.set_service(Some(Arc::new(service)));
            hotkeys::spawn_dispatcher(rx, st.hotkeys.clone(), st.cfg.clone(), st.dictation.clone());
        }
        Err(e) => {
            tracing::warn!("global shortcut unavailable: {e}");
            st.hotkeys.set_service(None);
        }
    }
}

/// Load the local models the settings use as soon as the app starts, and again whenever the settings start using
/// one that was not in use before. A model the user stopped by hand stays stopped: only newly selected ones start.
fn autostart_local_models(st: &Arc<AppState>) {
    use dito_core::factory::LocalBackend;
    use dito_core::health::EngineStatus;

    let (local, mut cfg) = (st.local.clone(), st.cfg.subscribe());
    st.rt.spawn(async move {
        let mut started = std::collections::BTreeSet::new();
        loop {
            let wanted = cfg.borrow_and_update().local_models_in_use();
            for id in wanted.difference(&started) {
                if local.status(id) == EngineStatus::Stopped {
                    let (local, id) = (local.clone(), id.clone());
                    tokio::spawn(async move { local.warm(&id).await });
                }
            }
            started = wanted;
            if cfg.changed().await.is_err() {
                break;
            }
        }
    });
}

/// Permissions are granted in System Settings while the app runs; the event tap cannot be created before that.
/// Try again every two seconds until it works, so the shortcut starts the moment the switch is turned on.
fn retry_hotkeys_when_allowed(st: Arc<AppState>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            if !st.hotkeys.available() {
                use dito_platform::{Permission, Status, permissions};
                let allowed = permissions::status(Permission::Accessibility) == Status::Granted;
                if allowed {
                    start_hotkeys(&st);
                }
            }
        }
    })
    .detach();
}

/// Headless end-to-end check: key down → finish → wait for delivery → print what was stored.
fn demo_run(st: Arc<AppState>, cx: &mut App) {
    use dito_core::controller::{Command, HudState};
    cx.spawn(async move |cx| {
        let mut hud = st.dictation.hud();
        cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
        st.dictation.send(Command::KeyDown);
        cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
        st.dictation.send(Command::Finish);
        let mut seen = Vec::new();
        let outcome = loop {
            if hud.changed().await.is_err() {
                break None;
            }
            let s = hud.borrow_and_update().clone();
            seen.push(format!("{s:?}"));
            if matches!(s, HudState::Inserted | HudState::Copied | HudState::Error { .. }) {
                break Some(s);
            }
        };
        println!("HUD states: {}", seen.join(" -> "));
        match st.store.list_history(&Default::default()).ok().and_then(|h| h.into_iter().next()) {
            Some(e) => println!(
                "history: kind={:?} app={:?} raw={:?} polished={:?} error={:?}",
                e.kind, e.app, e.raw, e.polished, e.error
            ),
            None => println!("history: (empty)"),
        }
        let ok = matches!(outcome, Some(HudState::Inserted | HudState::Copied));
        println!("demo-run: {}", if ok { "OK" } else { "FAILED" });
        cx.update(|cx| cx.quit());
        if !ok {
            std::process::exit(1);
        }
    })
    .detach();
}

fn set_app_menu(lang: dito_core::i18n::Lang, cx: &mut App) {
    cx.set_menus(vec![Menu {
        name: "Dito".into(),
        items: vec![MenuItem::action(lang.tr(dito_core::i18n::Tr::MenubarQuit), Quit)],
        disabled: false,
    }]);
}

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

    gpui_kit::application().with_assets(assets::AppAssets).run(move |cx| {
        gpui_kit::init(cx);
        dito_platform::system::set_app_icon(include_bytes!("../../../assets/icon/AppIcon-1024.png"));
        cx.text_system().add_fonts(assets::fonts()).expect("bundled fonts");
        ui::init_text_system(cx);
        cx.set_global(AppGlobal(st.clone()));

        // Release local models before the process exits (see `LocalEngines::unload_all`).
        let local = st.local.clone();
        cx.on_app_quit(move |_| {
            local.unload_all();
            async {}
        })
        .detach();
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        set_app_menu(st.cfg.get().general.lang(), cx);
        let mut config_rx = st.cfg.subscribe();
        cx.spawn(async move |cx| {
            let mut shown = config_rx.borrow().general.lang();
            while config_rx.changed().await.is_ok() {
                let lang = config_rx.borrow_and_update().general.lang();
                if lang != shown {
                    shown = lang;
                    cx.update(|cx| set_app_menu(lang, cx));
                }
            }
        })
        .detach();

        #[cfg(feature = "screenshots")]
        if let Some(dir) = st.opts.screenshots.clone() {
            shots::run(dir, cx);
            return;
        }

        if st.opts.demo_run {
            demo_run(st.clone(), cx);
            return;
        }
        start_hotkeys(&st);
        retry_hotkeys_when_allowed(st.clone(), cx);
        autostart_local_models(&st);
        hud::run(st.clone(), cx);
        menubar::run(st.clone(), cx);
        if !st.cfg.get().onboarded {
            onboarding::open(st.clone(), cx);
        } else if !st.opts.background {
            settings::open(st.clone(), settings::Page::General, cx);
        }
        // Ask while the settings window is up and the app active; NotDetermined only ever prompts once.
        if dito_platform::permissions::status(dito_platform::Permission::Microphone)
            == dito_platform::Status::NotDetermined
        {
            dito_platform::permissions::request(dito_platform::Permission::Microphone);
        }
    });
    Ok(())
}
