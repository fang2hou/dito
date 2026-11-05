//! Stand-in components for `--mock`, `--wav`, `--log-sink` and `--demo-run`: no microphone,
//! no keychain, no typing into other apps, plus the sample data a fresh mock data dir gets.

use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::Result;
use dito_core::audio::AudioClip;
use dito_core::controller::{Delivery, Recorder, Sink};
use dito_core::store::Store;

/// Plays a WAV file as "the microphone": whatever is recorded is that file. Lets the real engines be exercised
/// without a microphone, permissions or a speaker loop.
pub struct WavRecorder {
    clip: AudioClip,
    phase: AtomicU32,
}

impl WavRecorder {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
        let clip = AudioClip::from_wav(&bytes)
            .ok_or_else(|| anyhow::anyhow!("{} is not a 16-bit mono WAV", path.display()))?;
        Ok(Self { clip, phase: AtomicU32::new(0) })
    }
}

impl Recorder for WavRecorder {
    fn start(&self) -> Result<(), String> {
        self.phase.store(0, Ordering::Relaxed);
        Ok(())
    }
    fn stop(&self) -> AudioClip {
        self.clip.clone()
    }
    fn level(&self) -> f32 {
        let p = self.phase.fetch_add(1, Ordering::Relaxed) as f32;
        0.35 + 0.3 * (p * 0.35).sin().abs()
    }
}

/// Stand-in microphone: a steady tone, so the mock pipeline always has speech to "recognize".
pub struct SyntheticRecorder {
    phase: AtomicU32,
}

impl SyntheticRecorder {
    pub fn new() -> Self {
        Self { phase: AtomicU32::new(0) }
    }
}

impl Default for SyntheticRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder for SyntheticRecorder {
    fn start(&self) -> Result<(), String> {
        self.phase.store(0, Ordering::Relaxed);
        Ok(())
    }
    fn stop(&self) -> AudioClip {
        AudioClip::new((0..32_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect())
    }
    fn level(&self) -> f32 {
        let p = self.phase.fetch_add(1, Ordering::Relaxed) as f32;
        0.35 + 0.3 * (p * 0.35).sin().abs()
    }
}

pub(crate) struct LogSink;
impl Sink for LogSink {
    fn deliver(&self, text: &str, _: dito_core::config::InsertMethod) -> Delivery {
        tracing::info!("would insert: {text}");
        Delivery::Inserted
    }
}

/// Sample history and dictionary so a fresh mock run has something to show.
pub(crate) fn seed_demo(store: &Store, now: i64) -> Result<()> {
    use dito_core::prompts::Style;
    use dito_core::store::{HistoryKind, NewHistory};
    if !store.list_history(&Default::default())?.is_empty() {
        return Ok(());
    }
    for w in ["Acme", "Contoso", "gpui-kit", "サンプル株式会社", "Bedrock"] {
        store.add_word(w, now)?;
    }
    let mk = |ago: i64,
              kind,
              style,
              app: &str,
              lang: &str,
              dur: u64,
              raw: &str,
              polished: Option<&str>,
              error: Option<&str>| NewHistory {
        ts: now - ago,
        kind,
        style,
        app: app.into(),
        lang: lang.into(),
        duration_ms: dur,
        raw: raw.into(),
        polished: polished.map(Into::into),
        error: error.map(Into::into),
        audio_path: None,
    };
    let d = HistoryKind::Dictation;
    let items = [
        mk(
            600,
            d,
            Style::Slack,
            "Slack",
            "ja",
            9_000,
            "えーと 明日の3時の、あのレビュー会議 4時に変えて、で、えっと Dito のプロトタイプを 見といてください",
            Some("明日15時のレビュー会議は16時に変更します。事前にDitoのプロトタイプをご確認ください。"),
            None,
        ),
        mk(1_000, HistoryKind::Cancelled, Style::General, "Notes", "", 8_000, "", None, None),
        mk(
            1_400,
            d,
            Style::General,
            "Mail",
            "en",
            5_000,
            "um thanks I'll send the revised proposal Monday",
            Some("Thanks, I'll send the revised proposal Monday."),
            None,
        ),
        mk(
            1_900,
            d,
            Style::General,
            "WeChat",
            "zh",
            4_000,
            "请把周五的会议纪要发给我 谢谢",
            Some("请把周五的会议纪要发给我，谢谢。"),
            None,
        ),
        mk(
            2_300,
            d,
            Style::Teams,
            "Teams",
            "ja",
            6_000,
            "本日の定例は15分遅れで開始します",
            Some("本日の定例は15分遅れで開始します。"),
            None,
        ),
        mk(
            2_600,
            HistoryKind::Failed,
            Style::General,
            "Notes",
            "",
            3_000,
            "",
            None,
            Some("Auth: HTTP 401: invalid api key"),
        ),
    ];
    for it in items {
        store.add_history(&it)?;
    }
    let chars = [1200usize, 980, 1560, 1340, 1820, 560];
    for (i, n) in chars.iter().enumerate() {
        let text: String = "あ".repeat(*n);
        store.add_history(&mk(
            86_400 * (6 - i as i64),
            d,
            Style::General,
            ["Slack", "Gmail", "Notion", "Teams"][i % 4],
            if i % 3 == 2 { "en" } else { "ja" },
            (*n as u64) * 340,
            &text,
            Some(&text),
            None,
        ))?;
    }
    Ok(())
}
