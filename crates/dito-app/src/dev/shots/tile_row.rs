//! The configuration tile row on a card of its own, drawn as `20.05.13` shows it, and a scripted run of the row's
//! interactions with simulated pointer events. Opt-in: `DITO_SHOTS_ONLY=tilerow`.

use std::path::Path;
use std::time::Duration;

use dito_core::config::Appearance;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::*;

use super::capture::{close_window, render, save, theme_tag};
use super::input::{click, hover, redraw};
use crate::theme::{self, Palette};
use crate::ui::{self, Root};

const WIDTH: f32 = 774.;
const HEIGHT: f32 = 280.;
/// Where the row sits in the window: the card's 1 px border and its padding (26 and 20) in.
const ROW_LEFT: f32 = 27.;
const ROW_CENTRE: f32 = 53.;
/// The tiles' area is 672 wide: 720, less the 12 gap and the 36 plus button.
const VIEW_W: f32 = 672.;
const PLUS_CENTRE: f32 = ROW_LEFT + VIEW_W + 12. + 18.;
const TEMPLATES: [Tr; 3] = [Tr::ModelsOpenaiCompatible, Tr::ModelsProviderAzure, Tr::ModelsProviderBedrock];

struct Item {
    id: SharedString,
    name: SharedString,
    detail: SharedString,
}

fn item(id: &str, name: &str, detail: &str) -> Item {
    Item { id: id.to_string().into(), name: name.to_string().into(), detail: detail.to_string().into() }
}

/// The default configurations by their names, then two more.
fn samples() -> Vec<Item> {
    let name = |tr: Tr| Lang::Ja.tr(tr);
    vec![
        item("compat", name(Tr::ModelsOpenaiCompatible), "openai/gpt-4o-mini"),
        item("azure", name(Tr::ModelsProviderAzure), "gpt-6.1-terra"),
        item("bedrock", name(Tr::ModelsProviderBedrock), "apac.anthropic.claude-sonnet-4-20250514-v1:0"),
        item("long", "Long input", "openai/gpt-4.1"),
        item("lab", "Experiment", "openai/gpt-4.1-nano"),
    ]
}

/// A row of tiles on a card, with a plus menu and a log of what the row reported.
struct TileRowShot {
    p: Palette,
    state: ui::TileRowState,
    items: Vec<Item>,
    selected: SharedString,
    menu: bool,
    added: usize,
    log: Vec<String>,
    motion: ui::MotionScope,
}

impl TileRowShot {
    fn new(p: Palette) -> Self {
        let mut items = samples();
        items.truncate(3);
        let selected = items[0].id.clone();
        Self {
            p,
            state: ui::TileRowState::new(),
            items,
            selected,
            menu: false,
            added: 0,
            log: Vec::new(),
            motion: ui::MotionScope::new(),
        }
    }

    /// What the Models page does for a template: a new tile named for it joins the end and is selected.
    fn add(&mut self, template: &str, cx: &mut Context<Self>) {
        self.added += 1;
        let id = SharedString::from(format!("new-{}", self.added));
        let (name, detail) = (Lang::Ja.tr(Tr::ModelsNewConfiguration), Lang::Ja.tr(Tr::ModelsNoModel));
        self.items.push(item(&id, name, detail));
        self.selected = id;
        self.menu = false;
        self.log.push(format!("add from {template}"));
        cx.notify();
    }

    fn plus_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let items = TEMPLATES
            .iter()
            .map(|tr| {
                let label = Lang::Ja.tr(*tr);
                ui::MenuItem {
                    icon: None,
                    label: label.into(),
                    tone: ui::MenuTone::Default,
                    on_click: ui::cbk(cx, move |this: &mut Self, _, cx| this.add(label, cx)),
                }
            })
            .collect();
        let close = ui::cbk(cx, |this: &mut Self, _, cx| {
            this.menu = false;
            cx.notify();
        });
        ui::popup_menu_grouped("shot-plus-menu", &self.p, vec![items], close).into_any_element()
    }

    fn view(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let tiles = self
            .items
            .iter()
            .map(|i| ui::ConfigTile {
                id: i.id.clone(),
                name: i.name.clone(),
                detail: i.detail.clone(),
                selected: i.id == self.selected,
            })
            .collect();
        let row = ui::tile_row("shot-tiles", &p, &self.state, tiles)
            .on_select(ui::cbk_id(cx, |this: &mut Self, id, _, cx| {
                this.log.push(format!("select {id}"));
                this.selected = id.to_string().into();
                cx.notify();
            }))
            .plus(ui::TilePlus {
                open: self.menu,
                on_press: ui::cbk(cx, |this: &mut Self, _, cx| {
                    this.menu = !this.menu;
                    cx.notify();
                }),
                menu: self.menu.then(|| self.plus_menu(cx)),
            });
        div().size_full().bg(p.bg).font_family(ui::cjk_family()).child(ui::card(&p).px(px(26.)).py(px(20.)).child(row))
    }
}

impl Render for TileRowShot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        theme::apply(&self.p, cx);
        ui::set_ui_language(Lang::Ja);
        ui::with_motion_scope(self.motion.handle(), || self.view(cx))
    }
}

fn open_window(cx: &mut AsyncApp, p: Palette) -> Option<(WindowHandle<Root>, Entity<TileRowShot>)> {
    cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx))),
            titlebar: None,
            focus: false,
            show: false,
            ..Default::default()
        };
        let mut slot = None;
        let window = cx
            .open_window(options, |window, cx| {
                let view = cx.new(|_| TileRowShot::new(p));
                slot = Some(view.clone());
                cx.new(|cx| Root::new(view, window, cx))
            })
            .ok()?;
        Some((window, slot?))
    })
}

/// The scripted run on the row, in both appearances.
pub async fn tile_row_shots(dir: &Path, cx: &mut AsyncApp) {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let p = if appearance == Appearance::Dark { Palette::dark() } else { Palette::light() };
        Rig::open(dir, cx, p, theme_tag(appearance)).await.run().await;
    }
}

/// The row's window, with the pointer driven by simulated events.
struct Rig<'a> {
    cx: &'a mut AsyncApp,
    dir: &'a Path,
    tag: &'a str,
    window: WindowHandle<Root>,
    view: Entity<TileRowShot>,
}

impl<'a> Rig<'a> {
    async fn open(dir: &'a Path, cx: &'a mut AsyncApp, p: Palette, tag: &'a str) -> Self {
        let (window, view) = open_window(cx, p).expect("tile row window");
        let mut rig = Self { cx, dir, tag, window, view };
        rig.wait(300).await;
        rig
    }

    async fn wait(&mut self, ms: u64) {
        self.cx.background_executor().timer(Duration::from_millis(ms)).await;
    }

    async fn shot(&mut self, name: &str) {
        self.wait(60).await;
        if let Some(img) = render(self.cx, self.window) {
            save(self.dir, &format!("{name}-{}.png", self.tag), img);
        }
    }

    /// Run `f` against the window the way a platform event would, then draw.
    fn input(&mut self, f: impl FnOnce(&mut Window, &mut App)) {
        let window = self.window;
        self.cx.update(|cx| {
            cx.update_window(window.into(), |_, w, cx| {
                f(w, cx);
                redraw(w, cx);
            })
            .expect("tile row window update");
        });
    }

    /// Move the pointer off the row, so no tile shows the hover.
    fn park(&mut self) {
        self.input(|w, cx| hover(w, cx, point(px(8.), px(HEIGHT - 8.))));
    }

    fn model<R>(&mut self, f: impl FnOnce(&mut TileRowShot, &mut Context<TileRowShot>) -> R) -> R {
        let view = self.view.clone();
        self.cx.update(|cx| view.update(cx, f))
    }

    fn log(&mut self) -> Vec<String> {
        self.model(|v, _| v.log.clone())
    }

    fn selected(&mut self) -> String {
        self.model(|v, _| v.selected.to_string())
    }

    fn menu_open(&mut self) -> bool {
        self.model(|v, _| v.menu)
    }

    async fn run(mut self) {
        let tag = self.tag.to_string();
        self.park();
        self.shot("tile-row-3").await;

        // A tile lights up under the pointer.
        let second = point(px(ROW_LEFT + 216. + 12. + 100.), px(ROW_CENTRE));
        self.input(|w, cx| hover(w, cx, second));
        self.wait(250).await;
        self.shot("tile-row-3-hover").await;

        // A click selects.
        self.input(|w, cx| click(w, cx, second));
        assert_eq!((self.log().last().cloned(), self.selected()), (Some("select azure".into()), "azure".into()));
        self.park();
        self.wait(250).await;
        self.shot("tile-row-3-selected").await;

        // Four and more tiles keep their width.
        self.model(|v, cx| {
            v.items = samples();
            cx.notify();
        });
        self.wait(300).await;
        self.shot("tile-row-5-start").await;

        // The plus button hangs its menu under itself; a template adds a tile that is selected.
        let plus = point(px(PLUS_CENTRE), px(ROW_CENTRE));
        self.input(|w, cx| click(w, cx, plus));
        self.wait(250).await;
        self.shot("tile-row-plus-menu").await;
        self.input(|w, cx| click(w, cx, plus));
        self.wait(100).await;
        assert!(!self.menu_open(), "a press on the plus button while its menu is open closes it");
        self.input(|w, cx| click(w, cx, plus));
        self.wait(250).await;
        // Panel: 224 wide, right-aligned with the button, 44 under its top; first row 6 in, 34 high.
        let first_row = point(px(PLUS_CENTRE + 18. - 112.), px(ROW_CENTRE - 18. + 44. + 6. + 17.));
        self.input(|w, cx| click(w, cx, first_row));
        let template = Lang::Ja.tr(Tr::ModelsOpenaiCompatible);
        assert_eq!(self.log().last().cloned(), Some(format!("add from {template}")));
        assert_eq!(self.selected(), "new-1");
        println!("verified tile row hover, click and plus menu ({tag})");
        let window = self.window;
        close_window(self.cx, window);
    }
}
