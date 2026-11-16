use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::{Lang, Tr};
use dito_platform::hotkey::dictation_shortcut;
use dito_platform::{HotkeyService, Permission, Status, permissions};
use gpui_kit::*;

use crate::dev::demo_run;
use crate::state::{AppGlobal, AppState};
use crate::{hotkeys, hud, menubar, onboarding, settings};

actions!(dito, [Quit, OpenSettings, OpenHistory, OpenWordbook, OpenStatus, OpenTools]);

/// Needs no app state: the startup failure window runs it too.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    dito_platform::system::set_app_icon(include_bytes!("../../../assets/icon/AppIcon-1024.png"));
    cx.text_system().add_fonts(crate::assets::fonts()).expect("bundled fonts");
    crate::ui::init_text_system(cx);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
}

pub fn run(st: Arc<AppState>, cx: &mut App) {
    init(cx);
    launch(st, cx);
}

/// Runs once the services are built. That is after GPUI started when the startup failure window came first.
pub fn launch(st: Arc<AppState>, cx: &mut App) {
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
    follow_shortcuts(&st);
    open_tools_on_press(&st, cx);
    crate::services::serve_own_window_inserts(&st, cx);
    hud::run(st.clone(), cx);
    menubar::run(st.clone(), cx);
    open_first_window(&st, cx);
}

fn unload_local_models_on_quit(st: &AppState, cx: &mut App) {
    let local = st.local.clone();
    cx.on_app_quit(move |_| {
        local.unload_all();
        async {}
    })
    .detach();
}

fn install_app_menu(st: &Arc<AppState>, cx: &mut App) {
    on_open_page::<OpenSettings>(st, settings::Page::General, cx);
    on_open_page::<OpenHistory>(st, settings::Page::History, cx);
    on_open_page::<OpenWordbook>(st, settings::Page::Wordbook, cx);
    on_open_page::<OpenStatus>(st, settings::Page::Status, cx);
    on_open_tools(st, cx);
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

fn on_open_tools(st: &Arc<AppState>, cx: &mut App) {
    let st = st.clone();
    cx.on_action(move |_: &OpenTools, cx| crate::tools::open(st.clone(), cx));
}

fn set_menus(lang: Lang, cx: &mut App) {
    set_app_menu(lang, cx);
    set_dock_menu(lang, cx);
}

pub fn set_app_menu(lang: Lang, cx: &mut App) {
    cx.set_menus(vec![Menu {
        name: "Dito".into(),
        items: vec![MenuItem::action(lang.tr(Tr::MenubarQuit), Quit)],
        disabled: false,
    }]);
}

fn set_dock_menu(lang: Lang, cx: &mut App) {
    cx.set_dock_menu(vec![
        MenuItem::action(lang.tr(Tr::MenubarOpenTools), OpenTools),
        MenuItem::action(lang.tr(Tr::MenubarSettings), OpenSettings),
        MenuItem::action(lang.tr(Tr::HistoryOpen), OpenHistory),
        MenuItem::action(lang.tr(Tr::MenubarAddWord), OpenWordbook),
        MenuItem::action(lang.tr(Tr::NavStatus), OpenStatus),
    ]);
}

/// Starting Dito means the user wants to dictate, so every other window waits for a request from the menu bar, the
/// Dock or a second launch.
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

/// Does nothing while the startup failure window is all there is: there is no app state to open a page of.
pub fn reopen(cx: &mut App) {
    let Some(st) = cx.try_global::<AppGlobal>().map(|g| g.0.clone()) else { return };
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
    match HotkeyService::start(dictation_shortcut(&id)) {
        Ok((service, rx)) => {
            tracing::info!("shortcut tap live ({id})");
            st.hotkeys.set_service(Some(Arc::new(service)));
            let config = st.cfg.get();
            st.hotkeys.sync_command_shortcut(&config.general);
            st.hotkeys.sync_tools_shortcut(&config.general);
            hotkeys::spawn_dispatcher(rx, st.hotkeys.clone(), st.cfg.clone(), st.dictation.clone());
        }
        Err(e) => {
            tracing::warn!("global shortcut unavailable: {e}");
            st.hotkeys.set_service(None);
        }
    }
}

/// The command and Tools shortcuts follow the saved settings; the command one follows the dictation shortcut while it is
/// unset. No two of the three may be the same key.
fn follow_shortcuts(st: &Arc<AppState>) {
    let (hotkeys, mut cfg) = (st.hotkeys.clone(), st.cfg.subscribe());
    st.rt.spawn(async move {
        loop {
            let general = cfg.borrow_and_update().general.clone();
            hotkeys.sync_command_shortcut(&general);
            hotkeys.sync_tools_shortcut(&general);
            if cfg.changed().await.is_err() {
                break;
            }
        }
    });
}

/// The hotkey dispatcher thread has no window context, so a Tools press is drained here and opens the window.
fn open_tools_on_press(st: &Arc<AppState>, cx: &mut App) {
    let (st, mut pressed) = (st.clone(), st.hotkeys.subscribe_tools_pressed());
    cx.spawn(async move |cx| {
        while pressed.changed().await.is_ok() {
            cx.update(|cx| crate::tools::open(st.clone(), cx));
        }
    })
    .detach();
}

/// A model the user stopped by hand stays stopped: only newly selected ones start.
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

/// The event tap cannot be created until the user grants permission in System Settings, while the app runs.
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
