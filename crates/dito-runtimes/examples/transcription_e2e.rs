//! Hardware smoke test of transcription with the app's own parts: the system audio tap, Silero VAD, the local speech
//! and polish models, and the session actor. Play something on the Mac while it runs.
//!
//!   cargo run --release -p dito-runtimes --example transcription_e2e -- [seconds] [--mic | --mic-only] [--models DIR] [--file WAV] [--config TOML]
//!
//! Models come from the app's models folder (Silero VAD included); the app's config and history are not touched,
//! and polishing always runs on the local model. Run it from an app bundle that has `NSAudioCaptureUsageDescription`:
//! without it macOS records silence and never asks. `--file` plays a 16 kHz mono 16-bit WAV in real time as the
//! system audio instead, with no tap and no permission.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dito_audio::LiveCapture;
use dito_core::config::{ConfigStore, Source, default_data_dir};
use dito_core::controller::RecorderError;
use dito_core::factory::EngineFactory;
use dito_core::secrets::MemorySecrets;
use dito_core::store::{ParagraphState, Store};
use dito_core::transcription::{
    AudioSource, ChunkSink, LiveAudio, LiveInput, SessionState, Sources, TranscriptionDeps, TranscriptionStatus, spawn,
};

/// Prints the page's content whenever it changes.
#[derive(Default)]
struct Watch {
    last: String,
    /// Recognized words shown in rows or as settled live text, oldest first.
    settled: String,
    regressions: usize,
}

impl Watch {
    fn show(&mut self, at: f32, status: &TranscriptionStatus, store: &Store, id: i64) {
        let t = store.get_transcript(id).unwrap().expect("transcript");
        // As the page does: only the paragraphs the status says have closed.
        let live = status.state.transcript_id().is_some();
        let rows: Vec<_> = t
            .paragraphs
            .iter()
            .filter(|p| p.state != ParagraphState::Listening)
            .take(if live { status.closed } else { usize::MAX })
            .collect();
        let mut lines = Vec::new();
        for p in &rows {
            let state = if p.state == ParagraphState::Done { "done" } else { "polishing" };
            lines.push(format!("  row {:>5.1}s {state:9} {}", p.start_ms as f32 / 1000., p.text));
        }
        for l in &status.live {
            lines.push(format!(
                "  live {:?} {:>5.1}s {}[{}…]",
                l.speaker,
                l.start_ms as f32 / 1000.,
                l.settled,
                l.unsettled
            ));
        }
        let now = lines.join("\n");
        if now != self.last {
            println!("{at:6.2}s rev {} {:?}\n{now}", status.revision, status.state);
            self.last = now;
        }
        // Raw words in time order, then the live settled words of the newest paragraph.
        let mut settled: String = rows.iter().map(|p| p.raw.as_str()).collect();
        for l in &status.live {
            settled.push_str(&l.settled);
        }
        if !settled.starts_with(&self.settled) {
            self.regressions += 1;
            println!("!! settled text went back:\n   was {}\n   now {settled}", self.settled);
        }
        self.settled = settled;
    }
}

struct WavAudio(Vec<f32>);

struct Playing(Arc<AtomicBool>);
impl LiveInput for Playing {}
impl Drop for Playing {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl LiveAudio for WavAudio {
    fn open(&self, source: AudioSource, mut on_chunk: ChunkSink) -> Result<Box<dyn LiveInput>, RecorderError> {
        if source != AudioSource::System {
            return Err(RecorderError::NoInputDevice);
        }
        let (samples, stop) = (self.0.clone(), Arc::new(AtomicBool::new(false)));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            for chunk in samples.chunks(1_600) {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
                on_chunk(chunk);
            }
        });
        Ok(Box::new(Playing(stop)))
    }
}

fn read_wav(path: &std::path::Path) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("wav file");
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if &bytes[at..at + 4] == b"data" {
            let data = &bytes[at + 8..(at + 8 + len).min(bytes.len())];
            return data.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32_768.).collect();
        }
        at += 8 + len + len % 2;
    }
    panic!("no data chunk in {}", path.display());
}
use dito_local::{LocalEngines, ModelManager, SILERO_VAD};
use dito_runtimes::{Runtimes, SileroDetectors};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let secs: u64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(60);
    // `--mic` adds the microphone to the Mac's audio; `--mic-only` records the microphone alone.
    let mic_only = args.iter().any(|a| a == "--mic-only");
    let sources = Sources { system: !mic_only, microphone: mic_only || args.iter().any(|a| a == "--mic") };
    let models_dir =
        args.iter().position(|a| a == "--models").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    let file = args.iter().position(|a| a == "--file").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);

    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| default_data_dir().join("config.toml"));
    let app_config = ConfigStore::open(config_path).expect("app config");
    let dir = std::env::temp_dir().join("dito-transcription-e2e");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = Arc::new(ConfigStore::open(dir.join("config.toml")).unwrap());
    let mut copied = (*app_config.get()).clone();
    copied.models_dir = models_dir.or(copied.models_dir).or_else(|| Some(default_data_dir().join("models")));
    copied.text_model.source = Source::Local;
    copied.polish.enabled = true;
    cfg.update(|c| *c = copied).unwrap();

    let models = ModelManager::new(cfg.clone());
    assert!(models.is_installed(SILERO_VAD), "download {SILERO_VAD} from the Transcription page first");
    let local = LocalEngines::new(models.clone(), Arc::new(Runtimes));
    let engines = Arc::new(EngineFactory::new(Arc::new(MemorySecrets::default())).with_local(local));
    let store = Arc::new(Store::open(dir.join("e2e.db"), 0).unwrap());
    let audio: Arc<dyn LiveAudio> = match file {
        Some(path) => Arc::new(WavAudio(read_wav(&path))),
        None => Arc::new(LiveCapture::new(String::new)),
    };
    let handle = spawn(TranscriptionDeps {
        audio,
        detectors: Arc::new(SileroDetectors::new(&models.model_dir(SILERO_VAD).join("silero_vad.onnx"))),
        engines,
        config: cfg,
        store: store.clone(),
    });

    let mut rx = handle.subscribe();
    handle.start(sources);
    let started = rx.wait_for(|s| s.state.transcript_id().is_some() || s.error.is_some());
    let status = tokio::time::timeout(Duration::from_secs(180), started)
        .await
        .expect("session did not start")
        .expect("actor alive")
        .clone();
    let Some(id) = status.state.transcript_id() else {
        panic!("session failed to start: {:?}", status.error);
    };
    println!("recording transcript {id} for {secs} s");
    // What the page would show, each time it changes: closed paragraphs (from the store, as the page reads them on a
    // new revision) and the live text, with the seconds since the session started. Words settled once must never
    // go away again, which is checked as it runs.
    let begun = std::time::Instant::now();
    let deadline = tokio::time::sleep(Duration::from_secs(secs));
    tokio::pin!(deadline);
    let mut watch = Watch::default();
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            changed = rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let status = rx.borrow_and_update().clone();
                watch.show(begun.elapsed().as_secs_f32(), &status, &store, id);
            }
        }
    }
    handle.stop();
    loop {
        if rx.changed().await.is_err() {
            break;
        }
        let status = rx.borrow_and_update().clone();
        watch.show(begun.elapsed().as_secs_f32(), &status, &store, id);
        if status.state == SessionState::Idle {
            break;
        }
    }
    let done = rx.borrow().clone();
    println!("\n{} settled-text regressions", watch.regressions);
    if let Some(e) = done.error {
        println!("error: {e}");
    }
    let t = store.get_transcript(id).unwrap().expect("transcript");
    for p in t.paragraphs {
        let speaker = format!("{:?}", p.speaker);
        println!("\n[{} {speaker}] {:?}\n  raw:  {}\n  text: {}", p.start_ms / 1000, p.state, p.raw, p.text);
        if let Some(e) = p.error {
            println!("  polish error: {e}");
        }
    }
}
