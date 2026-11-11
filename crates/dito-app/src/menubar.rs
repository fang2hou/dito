//! Menu bar item: status, today's characters, shortcuts to pages. It never changes settings.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::Source;
use dito_core::health::Health;
use dito_core::i18n::Tr;
use gpui_kit::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::assets::tray_glyph;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::timefmt::{day_start, local_day, now};

const ID_HISTORY: &str = "history";
const ID_WORDBOOK: &str = "wordbook";
const ID_STATUS: &str = "status";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";

struct Items {
    status: MenuItem,
    today: MenuItem,
    history: MenuItem,
    wordbook: MenuItem,
    status_page: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

struct Tray {
    _icon: TrayIcon,
    items: Items,
}

fn accel(s: &str) -> Option<tray_icon::menu::accelerator::Accelerator> {
    s.parse().ok()
}

fn build() -> (Tray, Menu) {
    let menu = Menu::new();
    let status = MenuItem::new("…", false, None);
    let today = MenuItem::new("…", false, None);
    // Placeholder texts: refresh() (called right after build, and every ~2s) localizes every
    // item via t(), so the user only ever sees the localized strings.
    let history = MenuItem::with_id(ID_HISTORY, "…", true, accel("cmd+h"));
    let wordbook = MenuItem::with_id(ID_WORDBOOK, "…", true, accel("cmd+d"));
    let status_page = MenuItem::with_id(ID_STATUS, "…", true, accel("cmd+i"));
    let settings = MenuItem::with_id(ID_SETTINGS, "…", true, accel("cmd+,"));
    let quit = MenuItem::with_id(ID_QUIT, "…", true, accel("cmd+q"));
    let sep = || PredefinedMenuItem::separator();
    menu.append_items(&[&status, &today, &sep(), &history, &wordbook, &status_page, &sep(), &settings, &sep(), &quit])
        .expect("menu");
    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_icon_templated(tray_glyph())
        .with_tooltip("Dito")
        .build()
        .expect("tray icon");
    (Tray { _icon: icon, items: Items { status, today, history, wordbook, status_page, settings, quit } }, menu)
}

fn refresh(tray: &Tray, st: &AppState) {
    let cfg = st.cfg.get();
    let lang = cfg.general.lang();
    let health = Health::compute(&cfg, Some(&*st.local));
    let line = if !health.is_ok() {
        format!("● {}", lang.tr(Tr::StatusLocalStopped))
    } else if cfg.asr.source == Source::Local && (!cfg.polish.enabled || cfg.polish.engine.source == Source::Local) {
        format!("● {}", lang.tr(Tr::StatusRunningLocally))
    } else {
        format!("● {}", lang.tr(Tr::StatusRunningViaProvider))
    };
    tray.items.status.set_text(line);
    let off = dito_platform::local_utc_offset_secs();
    let since = day_start(local_day(now(), off), off);
    let chars = st.store.stats(Some(since)).map(|s| s.chars).unwrap_or(0);
    tray.items.today.set_text(format!("{} {} {}", lang.tr(Tr::HistoryToday), chars, lang.tr(Tr::UnitsChars)));
    tray.items.history.set_text(lang.tr(Tr::HistoryOpen));
    tray.items.wordbook.set_text(lang.tr(Tr::MenubarAddWord));
    tray.items.status_page.set_text(lang.tr(Tr::NavStatus));
    tray.items.settings.set_text(lang.tr(Tr::MenubarSettings));
    tray.items.quit.set_text(lang.tr(Tr::MenubarQuit));
}

/// Create the status item (must run on the main thread inside the GPUI app) and start handling its events.
pub fn run(st: Arc<AppState>, cx: &mut App) {
    let (tray, _menu) = build();
    refresh(&tray, &st);
    cx.spawn(async move |cx| {
        let events = MenuEvent::receiver();
        let mut tick = 0u32;
        loop {
            cx.background_executor().timer(Duration::from_millis(120)).await;
            while let Ok(ev) = events.try_recv() {
                let st2 = st.clone();
                let id = ev.id.0.clone();
                cx.update(|cx| match id.as_str() {
                    ID_HISTORY => settings::open(st2, Page::History, cx),
                    ID_WORDBOOK => settings::open(st2, Page::Wordbook, cx),
                    ID_STATUS => settings::open(st2, Page::Status, cx),
                    ID_SETTINGS => settings::open(st2, Page::General, cx),
                    ID_QUIT => cx.quit(),
                    _ => {}
                });
            }
            tick += 1;
            if tick.is_multiple_of(16) {
                refresh(&tray, &st);
            }
        }
    })
    .detach();
}
