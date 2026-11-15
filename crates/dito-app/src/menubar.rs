//! The menu bar item. It reports status and opens pages, and never changes settings.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::Source;
use dito_core::health::Health;
use dito_core::i18n::Tr;
use dito_platform::hotkey::{Mods, Shortcut, tools_shortcut};
use gpui_kit::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::assets::tray_glyph;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::timefmt::{day_start, local_day, now};

const ID_TOOLS: &str = "tools";
const ID_HISTORY: &str = "history";
const ID_WORDBOOK: &str = "wordbook";
const ID_STATUS: &str = "status";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";

struct Items {
    status: MenuItem,
    today: MenuItem,
    tools: MenuItem,
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

/// The shortcut the Open Tools item advertises: a combo of at least one modifier, in the menu toolkit's spelling of
/// the same key. A lone modifier, no shortcut at all, or a key the toolkit cannot name leaves the item without one.
fn tools_accelerator(id: &str) -> Option<tray_icon::menu::accelerator::Accelerator> {
    let shortcut = tools_shortcut(id)?;
    let Shortcut::Combo { mods, .. } = shortcut else { return None };
    if mods == Mods::default() {
        return None;
    }
    accel(&shortcut.id())
}

fn build() -> (Tray, Menu) {
    let menu = Menu::new();
    let status = MenuItem::new("…", false, None);
    let today = MenuItem::new("…", false, None);
    // Placeholder text: refresh() sets the localized text right after build, so the user never sees these.
    let tools = MenuItem::with_id(ID_TOOLS, "…", true, accel("ctrl+alt+t"));
    let history = MenuItem::with_id(ID_HISTORY, "…", true, accel("cmd+h"));
    let wordbook = MenuItem::with_id(ID_WORDBOOK, "…", true, accel("cmd+d"));
    let status_page = MenuItem::with_id(ID_STATUS, "…", true, accel("cmd+i"));
    let settings = MenuItem::with_id(ID_SETTINGS, "…", true, accel("cmd+,"));
    let quit = MenuItem::with_id(ID_QUIT, "…", true, accel("cmd+q"));
    let sep = || PredefinedMenuItem::separator();
    menu.append_items(&[
        &status,
        &today,
        &sep(),
        &tools,
        &history,
        &wordbook,
        &status_page,
        &sep(),
        &settings,
        &sep(),
        &quit,
    ])
    .expect("menu");
    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_icon_templated(tray_glyph())
        .with_tooltip("Dito")
        .build()
        .expect("tray icon");
    let items = Items { status, today, tools, history, wordbook, status_page, settings, quit };
    (Tray { _icon: icon, items }, menu)
}

fn refresh(tray: &Tray, st: &AppState) {
    let cfg = st.cfg.get();
    let lang = cfg.general.lang();
    let health = Health::compute(&cfg, Some(&*st.local));
    let line = if !health.is_ok() {
        format!("● {}", lang.tr(Tr::StatusLocalStopped))
    } else if cfg.asr.source == Source::Local && (!cfg.polish.enabled || cfg.text_model.source == Source::Local) {
        format!("● {}", lang.tr(Tr::StatusRunningLocally))
    } else {
        format!("● {}", lang.tr(Tr::StatusRunningViaProvider))
    };
    tray.items.status.set_text(line);
    let off = dito_platform::local_utc_offset_secs();
    let since = day_start(local_day(now(), off), off);
    let chars = st.store.stats(Some(since)).map(|s| s.chars).unwrap_or(0);
    tray.items.today.set_text(format!("{} {} {}", lang.tr(Tr::HistoryToday), chars, lang.tr(Tr::UnitsChars)));
    tray.items.tools.set_text(lang.tr(Tr::MenubarOpenTools));
    // A menu toolkit that cannot show it leaves the item without an accelerator.
    let _ = tray.items.tools.set_accelerator(tools_accelerator(&cfg.general.tools_shortcut));
    tray.items.history.set_text(lang.tr(Tr::HistoryOpen));
    tray.items.wordbook.set_text(lang.tr(Tr::MenubarAddWord));
    tray.items.status_page.set_text(lang.tr(Tr::NavStatus));
    tray.items.settings.set_text(lang.tr(Tr::MenubarSettings));
    tray.items.quit.set_text(lang.tr(Tr::MenubarQuit));
}

/// Must run on the main thread inside the GPUI app.
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
                    ID_TOOLS => crate::tools::open(st2, cx),
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
