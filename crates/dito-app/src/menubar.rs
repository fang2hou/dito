//! The menu bar item. It reports status and opens pages, and never changes settings.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{Config, Source};
use dito_core::health::{EngineStatus, Health};
use dito_core::i18n::Tr;
use dito_platform::hotkey::{Mods, Shortcut, tools_shortcut};
use gpui_kit::*;
use tokio::sync::mpsc;
use tray_icon::menu::accelerator::Accelerator;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::assets::tray_glyph;
use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::timefmt::{day_start, local_day, now};

const ID_TOOLS: &str = "tools";
const ID_HISTORY: &str = "history";
const ID_WORDBOOK: &str = "wordbook";
const ID_STATUS: &str = "status";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";

/// Whether a local engine runs has no change signal, so it is read this often while one is configured.
const ENGINE_POLL: Duration = Duration::from_secs(2);

/// A menu item that is written only when its text changes.
struct Label {
    item: MenuItem,
    text: String,
}

impl Label {
    fn new(item: MenuItem) -> Self {
        Self { item, text: String::new() }
    }

    fn set(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        if self.text != text {
            self.item.set_text(text);
            self.text = text.to_owned();
        }
    }
}

struct Items {
    status: Label,
    today: Label,
    tools: Label,
    history: Label,
    wordbook: Label,
    status_page: Label,
    settings: Label,
    quit: Label,
}

struct Tray {
    _icon: TrayIcon,
    items: Items,
    tools_accelerator: Option<Accelerator>,
}

enum Input {
    /// The count of the day may be out of date: the menu is about to open, or a new day began.
    Recount,
    Chosen(String),
}

fn accel(s: &str) -> Option<Accelerator> {
    s.parse().ok()
}

/// The shortcut the Open Tools item advertises: a combo of at least one modifier, in the menu toolkit's spelling of
/// the same key. A lone modifier, no shortcut at all, or a key the toolkit cannot name leaves the item without one.
fn tools_accelerator(id: &str) -> Option<Accelerator> {
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
    let tools_accelerator = accel("ctrl+alt+t");
    let tools = MenuItem::with_id(ID_TOOLS, "…", true, tools_accelerator);
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
    let items = Items {
        status: Label::new(status),
        today: Label::new(today),
        tools: Label::new(tools),
        history: Label::new(history),
        wordbook: Label::new(wordbook),
        status_page: Label::new(status_page),
        settings: Label::new(settings),
        quit: Label::new(quit),
    };
    (Tray { _icon: icon, items, tools_accelerator }, menu)
}

/// `chars` is the count of the day, `None` until the store has answered once.
fn refresh(tray: &mut Tray, cfg: &Config, health: &Health, chars: Option<u64>) {
    let lang = cfg.general.lang();
    let line = if !health.is_ok() {
        format!("● {}", lang.tr(Tr::StatusLocalStopped))
    } else if cfg.asr.source == Source::Local && (!cfg.polish.enabled || cfg.text_model.source == Source::Local) {
        format!("● {}", lang.tr(Tr::StatusRunningLocally))
    } else {
        format!("● {}", lang.tr(Tr::StatusRunningViaProvider))
    };
    tray.items.status.set(line);
    if let Some(chars) = chars {
        tray.items.today.set(format!("{} {} {}", lang.tr(Tr::HistoryToday), chars, lang.tr(Tr::UnitsChars)));
    }
    tray.items.tools.set(lang.tr(Tr::MenubarOpenTools));
    let accelerator = tools_accelerator(&cfg.general.tools_shortcut);
    if tray.tools_accelerator != accelerator {
        tray.tools_accelerator = accelerator;
        // A menu toolkit that cannot show it leaves the item without an accelerator.
        let _ = tray.items.tools.item.set_accelerator(accelerator);
    }
    tray.items.history.set(lang.tr(Tr::HistoryOpen));
    tray.items.wordbook.set(lang.tr(Tr::MenubarAddWord));
    tray.items.status_page.set(lang.tr(Tr::NavStatus));
    tray.items.settings.set(lang.tr(Tr::MenubarSettings));
    tray.items.quit.set(lang.tr(Tr::MenubarQuit));
}

/// The toolkit calls its handlers on the main thread, and keeps only the first handler it is given.
fn forward_tray_events(tx: mpsc::UnboundedSender<Input>) {
    let chosen = tx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = chosen.send(Input::Chosen(event.id.0));
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click { button_state: MouseButtonState::Down, .. } = event {
            let _ = tx.send(Input::Recount);
        }
    }));
}

fn today_start() -> i64 {
    let off = dito_platform::local_utc_offset_secs();
    day_start(local_day(now(), off), off)
}

fn announce_new_days(tx: mpsc::UnboundedSender<Input>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let until_tomorrow = (today_start() + 86_400 - now()).max(1) as u64;
            cx.background_executor().timer(Duration::from_secs(until_tomorrow)).await;
            if tx.send(Input::Recount).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Ends after `ENGINE_POLL` while a local engine is configured (one whose status is not `Remote`), and never otherwise.
async fn engine_poll(health: &Health, cx: &AsyncApp) {
    if health.asr != EngineStatus::Remote || health.polish != EngineStatus::Remote {
        cx.background_executor().timer(ENGINE_POLL).await;
    } else {
        std::future::pending().await
    }
}

/// Must run on the main thread inside the GPUI app, once per process.
pub fn run(st: Arc<AppState>, cx: &mut App) {
    let (tx, mut inputs) = mpsc::unbounded_channel();
    // The toolkit settles on its own channel at the first event it sends without a handler, so the handlers go in
    // before the icon exists.
    forward_tray_events(tx.clone());
    let (mut tray, _menu) = build();
    announce_new_days(tx, cx);
    let mut config = st.cfg.subscribe();
    let mut hud = st.dictation.hud();
    let cfg = st.cfg.get();
    refresh(&mut tray, &cfg, &Health::compute(&cfg, Some(&*st.local)), None);
    cx.spawn(async move |cx| {
        let mut counted = None;
        let mut recount = true;
        loop {
            let cfg = st.cfg.get();
            let health = Health::compute(&cfg, Some(&*st.local));
            refresh(&mut tray, &cfg, &health, counted.map(|(_, chars)| chars));
            let today = today_start();
            // The day is compared on every wake because a timer that slept with the machine announces the day late.
            if recount || counted.is_none_or(|(day, _)| day != today) {
                counted = Some((today, services::chars_since(st.clone(), today).await.unwrap_or(0)));
                recount = false;
                continue;
            }
            tokio::select! {
                biased;
                Some(input) = inputs.recv() => match input {
                    Input::Recount => recount = true,
                    Input::Chosen(id) => {
                        let st2 = st.clone();
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
                },
                Ok(()) = config.changed() => {}
                // The controller queues a dictation's history row before the HUD states that end the run.
                Ok(()) = hud.changed() => recount = true,
                () = engine_poll(&health, cx) => {}
            }
        }
    })
    .detach();
}
