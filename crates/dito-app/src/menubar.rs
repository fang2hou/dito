//! Menu bar item: status, today's characters, shortcuts to pages. It never changes settings.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::Source;
use dito_core::health::Health;
use dito_core::i18n::Tr;
use gpui_kit::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::settings::{self, Page};
use crate::state::AppState;
use crate::timefmt::{day_start, local_day, now};

const ID_HISTORY: &str = "history";
const ID_DICTIONARY: &str = "dictionary";
const ID_STATUS: &str = "status";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";

/// Five rounded waveform bars, drawn as a template (black + alpha) image.
fn glyph() -> Icon {
    let (w, h) = (44usize, 44usize);
    let mut rgba = vec![0u8; w * h * 4];
    let bars = [(8usize, 10usize), (14, 22), (20, 30), (26, 20), (32, 12)];
    for (i, (cx, bar_h)) in bars.into_iter().enumerate() {
        let _ = i;
        let (x0, x1) = (cx - 2, cx + 2);
        let (y0, y1) = ((h - bar_h) / 2, (h + bar_h) / 2);
        for y in y0..y1 {
            for x in x0..x1 {
                // round the ends
                let dy = if y < y0 + 2 {
                    y0 + 2 - y
                } else if y + 2 >= y1 {
                    y + 3 - y1
                } else {
                    0
                };
                if dy > 1 && (x == x0 || x == x1 - 1) {
                    continue;
                }
                let o = (y * w + x) * 4;
                rgba[o..o + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, w as u32, h as u32).expect("tray icon")
}

struct Items {
    status: MenuItem,
    today: MenuItem,
    history: MenuItem,
    dictionary: MenuItem,
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
    let dictionary = MenuItem::with_id(ID_DICTIONARY, "…", true, accel("cmd+d"));
    let status_page = MenuItem::with_id(ID_STATUS, "…", true, accel("cmd+i"));
    let settings = MenuItem::with_id(ID_SETTINGS, "…", true, accel("cmd+,"));
    let quit = MenuItem::with_id(ID_QUIT, "…", true, accel("cmd+q"));
    let sep = || PredefinedMenuItem::separator();
    menu.append_items(&[
        &status,
        &today,
        &sep(),
        &history,
        &dictionary,
        &status_page,
        &sep(),
        &settings,
        &sep(),
        &quit,
    ])
    .expect("menu");
    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_icon_templated(glyph())
        .with_tooltip("Dito")
        .build()
        .expect("tray icon");
    (Tray { _icon: icon, items: Items { status, today, history, dictionary, status_page, settings, quit } }, menu)
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
    tray.items.dictionary.set_text(lang.tr(Tr::MenubarAddWord));
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
                    ID_DICTIONARY => settings::open(st2, Page::Dictionary, cx),
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
