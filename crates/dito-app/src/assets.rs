//! Embedded assets: fonts, the icon sets used by the design, with gpui-kit's bundle as fallback.

use std::borrow::Cow;

use gpui_kit::assets::Assets as KitAssets;
use gpui_kit::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use tray_icon::Icon;

#[derive(RustEmbed)]
#[folder = "assets"]
struct Embedded;

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(f) = Embedded::get(path) {
            return Ok(Some(f.data));
        }
        KitAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out: Vec<SharedString> =
            Embedded::iter().filter(|p| p.starts_with(path)).map(|p| SharedString::from(p.to_string())).collect();
        out.extend(KitAssets.list(path)?);
        Ok(out)
    }
}

/// All bundled font files.
pub fn fonts() -> Vec<Cow<'static, [u8]>> {
    Embedded::iter()
        .filter(|p| p.starts_with("fonts/") && (p.ends_with(".ttf") || p.ends_with(".otf")))
        .filter_map(|p| Embedded::get(&p).map(|f| f.data))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ic {
    Mic,
    BookOpen,
    Sparkles,
    Sliders,
    Cpu,
    Search,
    Plus,
    X,
    Check,
    ChevronDown,
    Mouse,
    MouseLeftButton,
    Trash,
    Globe,
    Download,
    Upload,
    Reset,
    ArrowLeft,
    ArrowRight,
    Undo,
    Redo,
    ArrowThin,
    Copy,
    Alert,
    Lock,
    Zap,
    Clock,
    Activity,
    MoreV,
    Refresh,
    External,
    Chart,
    Pause,
    Play,
    Stop,
    Ban,
}

impl Ic {
    pub fn path(self) -> &'static str {
        match self {
            Self::Mouse => "icons/lucide/mouse.svg",
            Self::MouseLeftButton => "icons/lucide/mouse-left-button.svg",
            Self::Mic => "icons/lucide/mic.svg",
            Self::BookOpen => "icons/lucide/book-open.svg",
            Self::Sparkles => "icons/lucide/sparkles.svg",
            Self::Sliders => "icons/lucide/sliders-horizontal.svg",
            Self::Cpu => "icons/lucide/cpu.svg",
            Self::Search => "icons/lucide/search.svg",
            Self::Plus => "icons/lucide/plus.svg",
            Self::X => "icons/lucide/x.svg",
            Self::Check => "icons/lucide/check.svg",
            Self::ChevronDown => "icons/lucide/chevron-down.svg",
            Self::Trash => "icons/lucide/trash-2.svg",
            Self::Globe => "icons/lucide/globe.svg",
            Self::Download => "icons/lucide/download.svg",
            Self::Upload => "icons/lucide/upload.svg",
            Self::Reset => "icons/lucide/rotate-ccw.svg",
            Self::Undo => "icons/lucide/undo-2.svg",
            Self::Redo => "icons/lucide/redo-2.svg",
            Self::ArrowLeft => "icons/lucide/arrow-left.svg",
            Self::ArrowRight => "icons/lucide/arrow-right.svg",
            Self::ArrowThin => "icons/lucide/arrow-thin.svg",
            Self::Copy => "icons/lucide/copy.svg",
            Self::Alert => "icons/lucide/triangle-alert.svg",
            Self::Lock => "icons/lucide/lock.svg",
            Self::Zap => "icons/lucide/zap.svg",
            Self::Clock => "icons/lucide/clock.svg",
            Self::Activity => "icons/lucide/activity.svg",
            Self::MoreV => "icons/lucide/ellipsis-vertical.svg",
            Self::Refresh => "icons/lucide/refresh-cw.svg",
            Self::External => "icons/lucide/external-link.svg",
            Self::Chart => "icons/lucide/chart-column.svg",
            Self::Pause => "icons/lucide/pause.svg",
            Self::Stop => "icons/lucide/square.svg",
            Self::Play => "icons/lucide/play.svg",
            Self::Ban => "icons/lucide/ban.svg",
        }
    }
}

/// Canonical HUD glyphs exported from the final FigCraft layers, including their native icon boxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudIc {
    X,
    Finish,
    Check,
    Retry,
    Sliders,
    Copy,
    Stop,
}

impl HudIc {
    pub fn path(self) -> &'static str {
        match self {
            Self::X => "icons/hud/x.svg",
            Self::Finish => "icons/hud/finish.svg",
            Self::Check => "icons/hud/check.svg",
            Self::Retry => "icons/hud/retry.svg",
            Self::Sliders => "icons/hud/sliders.svg",
            Self::Copy => "icons/hud/copy.svg",
            Self::Stop => "icons/hud/stop.svg",
        }
    }
}

const TRAY_GLYPH_SIZE: usize = 44;
/// Center column and height of each waveform bar.
const TRAY_GLYPH_BARS: [(usize, usize); 5] = [(8, 10), (14, 22), (20, 30), (26, 20), (32, 12)];
const TRAY_GLYPH_BAR_HALF_WIDTH: usize = 2;
const TRAY_GLYPH_ROUNDING: usize = 2;

/// Five rounded waveform bars, drawn as a template (black + alpha) image.
pub fn tray_glyph() -> Icon {
    let size = TRAY_GLYPH_SIZE;
    let mut rgba = vec![0u8; size * size * 4];
    for (center, bar_h) in TRAY_GLYPH_BARS {
        let (x0, x1) = (center - TRAY_GLYPH_BAR_HALF_WIDTH, center + TRAY_GLYPH_BAR_HALF_WIDTH);
        let (y0, y1) = ((size - bar_h) / 2, (size + bar_h) / 2);
        for y in y0..y1 {
            let inset = bar_end_inset(y, y0, y1);
            for x in x0..x1 {
                if inset > 1 && (x == x0 || x == x1 - 1) {
                    continue;
                }
                let offset = (y * size + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, size as u32, size as u32).expect("tray icon")
}

/// How far into the rounded cap row `y` lies: zero in the straight part of the bar.
fn bar_end_inset(y: usize, top: usize, bottom: usize) -> usize {
    if y < top + TRAY_GLYPH_ROUNDING {
        top + TRAY_GLYPH_ROUNDING - y
    } else if y + TRAY_GLYPH_ROUNDING >= bottom {
        y + TRAY_GLYPH_ROUNDING + 1 - bottom
    } else {
        0
    }
}
