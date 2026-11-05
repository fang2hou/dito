//! Embedded assets: fonts, the icon sets used by the design, with gpui-kit's bundle as fallback.

use std::borrow::Cow;

use gpui_kit::assets::Assets as KitAssets;
use gpui_kit::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;

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
