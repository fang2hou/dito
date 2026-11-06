use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::{Lang, Tr};
use dito_platform::{HotkeyService, Permission, Shortcut, Status, permissions};
use gpui_kit::*;

use crate::dev::demo_run;
use crate::state::{AppGlobal, AppState};
use crate::{hotkeys, hud, menubar, onboarding, settings};

actions!(dito, [Quit]);

/// Wire the global state, the application menu and the first windows once GPUI is running.
pub fn run(st: Arc<AppState>, cx: &mut App) {
    gpui_kit::init(cx);
    dito_platform::system::set_app_icon(include_bytes!("../../../assets/icon/AppIcon-1024.png"));
    cx.text_system().add_fonts(crate::assets::fonts()).expect("bundled fonts");
    crate::ui::init_text_system(cx);
    cx.set_global(AppGlobal(st.clone()));

    unload_local_models_on_quit(&st, cx);
    install_app_menu(&st, cx);

    #[cfg(feature = "screenshots")]
    if let Some(dir) = st.opts.screenshots.clone() {
        crate::dev::shots::run(dir, cx);
        return;
    }

    if st.opts.demo_run {
        demo_run::run(st, cx);
        return;
    }
    start_hotkeys(&st);
    retry_hotkeys_when_allowed(st.clone(), cx);
    autostart_local_models(&st);
    hud::run(st.clone(), cx);
    menubar::run(st.clone(), cx);
    open_first_window(&st, cx);
    request_microphone_access();
}

/// Release local models before the process exits (see `LocalEngines::unload_all`).
fn unload_local_models_on_quit(st: &AppState, cx: &mut App) {
    let local = st.local.clone();
    cx.on_app_quit(move |_| {
        local.unload_all();
        async {}
    })
    .detach();
}

fn install_app_menu(st: &AppState, cx: &mut App) {
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
}

fn set_app_menu(lang: Lang, cx: &mut App) {
    cx.set_menus(vec![Menu {
        name: "Dito".into(),
        items: vec![MenuItem::action(lang.tr(Tr::MenubarQuit), Quit)],
        disabled: false,
    }]);
}

fn open_first_window(st: &Arc<AppState>, cx: &mut App) {
    if !st.cfg.get().onboarded {
        onboarding::open(st.clone(), cx);
    } else if !st.opts.background {
        settings::open(st.clone(), settings::Page::General, cx);
    }
}

/// Ask while the settings window is up and the app active; NotDetermined only ever prompts once.
fn request_microphone_access() {
    if permissions::status(Permission::Microphone) == Status::NotDetermined {
        permissions::request(Permission::Microphone);
    }
}

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
    let (local, mut cfg) = (st.local.clone(), st.cfg.subscribe());
    st.rt.spawn(async move {
        let mut started = BTreeSet::new();
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
            cx.background_executor().timer(Duration::from_secs(2)).await;
            if !st.hotkeys.available() && permissions::status(Permission::Accessibility) == Status::Granted {
                start_hotkeys(&st);
            }
        }
    })
    .detach();
}
