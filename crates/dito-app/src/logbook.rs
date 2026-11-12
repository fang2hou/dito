//! The app's log lines since launch, kept in memory for the Status page, and the switch for the detailed log.
//!
//! One `EnvFilter` gates everything that is logged, for the terminal and for this buffer alike, so both show the same
//! lines. The switch rebuilds that filter at runtime: off keeps the default levels, on adds the `dito_trace` debug
//! lines. Nothing here is written to disk or sent anywhere, and the switch lives for the session only.

use std::collections::VecDeque;
use std::fmt::{self, Write as _};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError};

use anyhow::Result;
use dito_core::trace::TARGET as TRACE_TARGET;
use tokio::sync::watch;
use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::{Event, Id, Level, Subscriber};
use tracing_subscriber::filter::{EnvFilter, LevelFilter, Targets};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry, reload};

use crate::timefmt;

/// The newest lines kept; the oldest is dropped first. A run writes about fifteen lines with the detailed log on, so
/// this holds the last sixty or so, and a bound keeps the longest prompts from growing the process without limit.
pub const CAPACITY: usize = 1000;

/// Only these targets (and their sub-targets) are the app's own lines. `dito_audio` and `dito_trace` share the prefix.
const OWN_TARGET: &str = "dito";

const LEVEL_WIDTH: usize = 5;
const RUN_WIDTH: usize = 7;
const NO_RUN: &str = "-";
const COLUMN_GAP: &str = "  ";

/// One event as a line of text: `time  LEVEL  run  message  field=value`, with the head columns padded so that the
/// messages line up in a monospaced face.
pub struct LogLine {
    text: String,
    head: usize,
    level: Range<usize>,
    severe: bool,
}

impl LogLine {
    fn new(time: &str, level: Level, run: Option<&str>, body: &str) -> Self {
        let mut text = String::with_capacity(time.len() + 24 + body.len());
        text.push_str(time);
        text.push_str(COLUMN_GAP);
        let level_start = text.len();
        let _ = write!(text, "{:<LEVEL_WIDTH$}", level.as_str());
        let level_end = text.len();
        text.push_str(COLUMN_GAP);
        let _ = write!(text, "{:<RUN_WIDTH$}", run.unwrap_or(NO_RUN));
        text.push_str(COLUMN_GAP);
        let head = text.len();
        text.push_str(&body.replace('\n', "\\n"));
        Self { text, head, level: level_start..level_end, severe: level == Level::WARN || level == Level::ERROR }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Bytes of the time, level and run id columns, which are shown muted.
    pub fn head_len(&self) -> usize {
        self.head
    }

    /// Where the level sits inside the head.
    pub fn level_range(&self) -> Range<usize> {
        self.level.clone()
    }

    /// Warnings and errors, shown in the danger color.
    pub fn is_severe(&self) -> bool {
        self.severe
    }
}

struct Control {
    handle: reload::Handle<EnvFilter, Registry>,
    env: Option<String>,
}

pub struct LogBook {
    capacity: usize,
    lines: Mutex<VecDeque<Arc<LogLine>>>,
    changed: watch::Sender<u64>,
    detail: AtomicBool,
    control: OnceLock<Control>,
}

impl LogBook {
    fn with_capacity(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            capacity,
            lines: Mutex::new(VecDeque::new()),
            changed: watch::channel(0).0,
            detail: AtomicBool::new(false),
            control: OnceLock::new(),
        })
    }

    /// The one buffer of the process, the same whether or not logging was installed.
    pub fn shared() -> Arc<Self> {
        static SHARED: LazyLock<Arc<LogBook>> = LazyLock::new(|| LogBook::with_capacity(CAPACITY));
        SHARED.clone()
    }

    pub fn lines(&self) -> Vec<Arc<LogLine>> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect()
    }

    /// Ticks after every recorded line.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// Whether the `dito_trace` debug lines are recorded and shown.
    pub fn detail(&self) -> bool {
        self.detail.load(Ordering::Relaxed)
    }

    /// Takes effect at once, for the terminal output as well. Lines already recorded stay.
    pub fn set_detail(&self, on: bool) {
        self.detail.store(on, Ordering::Relaxed);
        if let Some(control) = self.control.get()
            && let Err(error) = control.handle.reload(filter(control.env.as_deref(), on))
        {
            tracing::warn!("could not switch the detailed log: {error}");
        }
    }

    fn push(&self, line: LogLine) {
        {
            let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
            while lines.len() >= self.capacity {
                lines.pop_front();
            }
            lines.push_back(Arc::new(line));
        }
        self.changed.send_modify(|ticks| *ticks = ticks.wrapping_add(1));
    }

    /// The filter layer that gates every other layer. `RUST_LOG=dito_trace=debug` starts with the switch on.
    fn filter_layer(&self, env: Option<String>) -> reload::Layer<EnvFilter, Registry> {
        let on = env_enables_detail(env.as_deref());
        self.detail.store(on, Ordering::Relaxed);
        let (layer, handle) = reload::Layer::new(filter(env.as_deref(), on));
        let _ = self.control.set(Control { handle, env });
        layer
    }
}

/// The environment's directives first, then the app's defaults over them. The `dito_trace` directive is always set, so
/// that the switch decides even when `RUST_LOG` says otherwise.
fn filter(env: Option<&str>, detail: bool) -> EnvFilter {
    let trace = if detail { "dito_trace=debug" } else { "dito_trace=info" };
    let base = EnvFilter::builder().with_default_directive(LevelFilter::ERROR.into()).parse_lossy(env.unwrap_or(""));
    ["dito=info", "dito_audio=debug", trace]
        .into_iter()
        .fold(base, |filter, directive| filter.add_directive(directive.parse().expect("a fixed directive")))
}

/// Whether `RUST_LOG` asks for the `dito_trace` debug lines, judged with the app's defaults laid over it as in
/// [`filter`], so `dito=debug` alone does not count.
fn env_enables_detail(env: Option<&str>) -> bool {
    env.and_then(|value| value.parse::<Targets>().ok()).is_some_and(|targets| {
        targets
            .with_target("dito", LevelFilter::INFO)
            .with_target("dito_audio", LevelFilter::DEBUG)
            .would_enable(TRACE_TARGET, &Level::DEBUG)
    })
}

/// Terminal output and the in-app buffer, both behind the switchable filter.
pub fn install() -> Result<()> {
    let book = LogBook::shared();
    tracing_subscriber::registry()
        .with(book.filter_layer(std::env::var("RUST_LOG").ok()))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(Capture(book))
        .try_init()?;
    Ok(())
}

struct Capture(Arc<LogBook>);

/// The run id of a span that carries one.
struct RunTag(String);

#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
    run: Option<String>,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        match field.name() {
            "message" => {
                let _ = write!(self.message, "{value:?}");
            }
            "run" => self.run = Some(format!("{value:?}")),
            name => {
                let _ = write!(self.rest, "{COLUMN_GAP}{name}={value:?}");
            }
        }
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        if let (Some(run), Some(span)) = (fields.run, ctx.span(id)) {
            span.extensions_mut().insert(RunTag(run));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        if !meta.target().starts_with(OWN_TARGET) {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let run = fields.run.or_else(|| {
            ctx.event_scope(event)?.find_map(|span| span.extensions().get::<RunTag>().map(|tag| tag.0.clone()))
        });
        let body = format!("{}{}", fields.message, fields.rest);
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        let time = timefmt::clock_millis(now, dito_platform::local_utc_offset_secs());
        self.0.push(LogLine::new(&time, *meta.level(), run.as_deref(), body.trim_start()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded(book: &Arc<LogBook>, emit: impl FnOnce()) -> Vec<String> {
        let subscriber = tracing_subscriber::registry().with(book.filter_layer(None)).with(Capture(book.clone()));
        tracing::subscriber::with_default(subscriber, emit);
        book.lines().iter().map(|line| line.text().to_owned()).collect()
    }

    #[test]
    fn the_buffer_drops_the_oldest_line_at_its_bound() {
        let book = LogBook::with_capacity(3);
        let lines = recorded(&book, || {
            for n in 1..=5 {
                tracing::info!(n, "line");
            }
        });
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("line  n=3"), "{lines:?}");
        assert!(lines[2].ends_with("line  n=5"), "{lines:?}");
    }

    #[test]
    fn trace_lines_follow_the_switch_and_carry_their_run() {
        let book = LogBook::with_capacity(10);
        let subscriber = tracing_subscriber::registry().with(book.filter_layer(None)).with(Capture(book.clone()));
        tracing::subscriber::with_default(subscriber, || {
            let id = dito_core::trace::RunId::new('c');
            let emit = |tag: &str| {
                let _entered = dito_core::trace::run_span(&id).entered();
                dito_core::trace_log!(tag, "step");
                tracing::debug!("not ours, below the default level");
                tracing::warn!(target: "dito", "careful");
                tracing::info!(target: "somewhere_else", "ignored");
            };
            emit("off");
            book.set_detail(true);
            emit("on");
            book.set_detail(false);
            emit("off again");
        });
        let lines: Vec<_> = book.lines().iter().map(|line| line.text().to_owned()).collect();
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert!(lines[0].contains("WARN ") && lines[0].contains("  -        careful"), "{lines:?}");
        assert!(
            lines[1].contains("DEBUG") && lines[1].contains("  c") && lines[1].ends_with("step  tag=\"on\""),
            "{lines:?}"
        );
        assert!(lines[2].contains("WARN ") && lines[2].contains("careful"), "{lines:?}");
        assert!(lines[3].contains("WARN "), "{lines:?}");
    }

    #[test]
    fn the_environment_can_start_with_the_detailed_log_on() {
        assert!(env_enables_detail(Some("dito_trace=debug")));
        assert!(env_enables_detail(Some("info,dito_trace=trace")));
        assert!(!env_enables_detail(Some("dito=debug")));
        assert!(!env_enables_detail(None));
    }

    #[test]
    fn a_line_keeps_its_columns_and_marks_warnings() {
        let line = LogLine::new("10:44:38.020", Level::WARN, Some("d13"), "polish failed  kind=timeout");
        assert_eq!(line.text(), "10:44:38.020  WARN   d13      polish failed  kind=timeout");
        assert_eq!(&line.text()[line.level_range()], "WARN ");
        assert_eq!(&line.text()[line.head_len()..], "polish failed  kind=timeout");
        assert!(line.is_severe());
        assert!(!LogLine::new("00:00:00.000", Level::INFO, None, "x").is_severe());
    }
}
