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

actions!(dito, [Quit, OpenSettings, OpenHistory, OpenDictionary, OpenStatus]);

/// Wire the global state, the application menu and the first windows once GPUI is running.
pub fn run(st: Arc<AppState>, cx: &mut App) {
    gpui_kit::init(cx);
    dito_platform::system::set_app_icon(include_bytes!("../../../assets/icon/AppIcon-1024.png"));
    cx.text_system().add_fonts(crate::assets::fonts()).expect("bundled fonts");
    crate::ui::init_text_system(cx);
    cx.set_global(AppGlobal(st.clone()));

    unload_local_models_on_quit(&st, cx);
    install_app_menu(&st, cx);
    crate::dock::run(st.clone(), cx);

    #[cfg(feature = "screenshots")]
    if let Some(dir) = st.opts.screenshots.clone() {
        crate::dev::shots::run(dir, cx);
        return;
    }

    if st.opts.demo_run {
        demo_run::run(st, cx);
        return;
    }
    if permissions::status(Permission::Accessibility) == Status::Granted {
        start_hotkeys(&st);
    } else {
        st.hotkeys.set_service(None);
    }
    retry_hotkeys_when_allowed(st.clone(), cx);
    autostart_local_models(&st);
    hud::run(st.clone(), cx);
    menubar::run(st.clone(), cx);
    open_first_window(&st, cx);
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

fn install_app_menu(st: &Arc<AppState>, cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    on_open_page::<OpenSettings>(st, settings::Page::General, cx);
    on_open_page::<OpenHistory>(st, settings::Page::History, cx);
    on_open_page::<OpenDictionary>(st, settings::Page::Dictionary, cx);
    on_open_page::<OpenStatus>(st, settings::Page::Status, cx);
    set_menus(st.cfg.get().general.lang(), cx);
    let mut config_rx = st.cfg.subscribe();
    cx.spawn(async move |cx| {
        let mut shown = config_rx.borrow().general.lang();
        while config_rx.changed().await.is_ok() {
            let lang = config_rx.borrow_and_update().general.lang();
            if lang != shown {
                shown = lang;
                cx.update(|cx| set_menus(lang, cx));
            }
        }
    })
    .detach();
}

fn on_open_page<A: Action>(st: &Arc<AppState>, page: settings::Page, cx: &mut App) {
    let st = st.clone();
    cx.on_action(move |_: &A, cx| settings::open(st.clone(), page, cx));
}

/// The application menu holds only Quit; the Dock menu repeats the menu bar item's page shortcuts, Settings first.
fn set_menus(lang: Lang, cx: &mut App) {
    cx.set_menus(vec![Menu {
        name: "Dito".into(),
        items: vec![MenuItem::action(lang.tr(Tr::MenubarQuit), Quit)],
        disabled: false,
    }]);
    cx.set_dock_menu(vec![
        MenuItem::action(lang.tr(Tr::MenubarSettings), OpenSettings),
        MenuItem::action(lang.tr(Tr::HistoryOpen), OpenHistory),
        MenuItem::action(lang.tr(Tr::MenubarAddWord), OpenDictionary),
        MenuItem::action(lang.tr(Tr::NavStatus), OpenStatus),
    ]);
}

/// The window a launch opens by itself. Starting Dito means the user wants to dictate, so everything else waits
/// for a request from the menu bar, the Dock or a second launch.
#[derive(Debug, PartialEq, Eq)]
enum LaunchWindow {
    Onboarding,
    Settings(settings::Page),
}

fn launch_window(
    onboarded: bool,
    open_settings: bool,
    accessibility: Status,
    microphone: Status,
) -> Option<LaunchWindow> {
    if !onboarded {
        Some(LaunchWindow::Onboarding)
    } else if accessibility != Status::Granted {
        Some(LaunchWindow::Settings(settings::Page::General))
    } else if microphone == Status::Denied {
        Some(LaunchWindow::Settings(settings::Page::Status))
    } else if open_settings {
        Some(LaunchWindow::Settings(settings::Page::General))
    } else {
        None
    }
}

/// Clicking the Dock icon, or opening Dito again from Finder or Spotlight, while it runs with no window open.
pub fn reopen(st: Arc<AppState>, cx: &mut App) {
    if st.opts.is_headless() {
        return;
    }
    if st.cfg.get().onboarded {
        settings::open(st, settings::Page::General, cx);
    } else {
        onboarding::restart(st, cx);
    }
}

fn open_first_window(st: &Arc<AppState>, cx: &mut App) {
    let window = launch_window(
        st.cfg.get().onboarded,
        st.opts.open_settings,
        permissions::status(Permission::Accessibility),
        permissions::status(Permission::Microphone),
    );
    match window {
        Some(LaunchWindow::Onboarding) => onboarding::open(st.clone(), cx),
        Some(LaunchWindow::Settings(page)) => settings::open(st.clone(), page, cx),
        None => {}
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

#[cfg(test)]
mod tests {
    use dito_platform::Status;

    use super::{LaunchWindow, launch_window};
    use crate::settings;

    const OK: Status = Status::Granted;

    #[test]
    fn a_normal_launch_opens_nothing() {
        assert_eq!(launch_window(true, false, OK, OK), None);
        assert_eq!(launch_window(true, false, OK, Status::NotDetermined), None, "the system asks on first recording");
    }

    #[test]
    fn first_run_opens_onboarding_whatever_the_permissions() {
        assert_eq!(launch_window(false, false, OK, OK), Some(LaunchWindow::Onboarding));
        assert_eq!(launch_window(false, false, Status::Denied, Status::Denied), Some(LaunchWindow::Onboarding));
    }

    #[test]
    fn a_missing_permission_opens_the_page_that_explains_it() {
        let general = Some(LaunchWindow::Settings(settings::Page::General));
        let status = Some(LaunchWindow::Settings(settings::Page::Status));
        assert_eq!(launch_window(true, false, Status::Denied, OK), general);
        assert_eq!(launch_window(true, false, OK, Status::Denied), status);
        assert_eq!(
            launch_window(true, false, Status::Denied, Status::Denied),
            general,
            "the General banner comes first"
        );
    }

    #[test]
    fn the_dev_flag_opens_settings() {
        assert_eq!(launch_window(true, true, OK, OK), Some(LaunchWindow::Settings(settings::Page::General)));
    }
}
