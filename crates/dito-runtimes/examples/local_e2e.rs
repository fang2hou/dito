//! Real on-device check: download SenseVoice + a Qwen3 model, synthesize Japanese speech with macOS `say`,
//! recognize it, and polish the result.
//!
//!   cargo run --release -p dito-runtimes --example local_e2e -- /tmp/dito-models qwen3.5-2b-instruct
//!
//! Needs network access for the first download (models are cached in the given directory).

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::audio::AudioClip;
use dito_core::config::ConfigStore;
use dito_core::factory::LocalBackend;
use dito_core::prompts::{PromptInput, Prompts, Style, build_system_prompt};
use dito_core::provider::{AsrRequest, PolishRequest};
use dito_local::{LocalEngines, ModelManager, ModelState};
use dito_runtimes::Runtimes;

async fn ensure(mgr: &ModelManager, id: &str) {
    if mgr.is_installed(id) {
        return;
    }
    println!("downloading {id} ...");
    mgr.start_download(id).expect("start download");
    let mut rx = mgr.subscribe();
    let mut last = Instant::now();
    loop {
        let state = rx.borrow_and_update().get(id).cloned().unwrap_or(ModelState::NotInstalled);
        match state {
            ModelState::Installed => break,
            ModelState::Failed { message, .. } => panic!("download of {id} failed: {message}"),
            ModelState::Downloading { done, total, bytes_per_sec } if last.elapsed() > Duration::from_secs(5) => {
                last = Instant::now();
                println!("  {id}: {:.0}% ({:.1} MB/s)", done as f64 * 100.0 / total as f64, bytes_per_sec / 1e6);
            }
            _ => {}
        }
        if tokio::time::timeout(Duration::from_secs(30), rx.changed()).await.is_err() {
            continue;
        }
    }
}

fn synthesize(text: &str) -> AudioClip {
    let dir = std::env::temp_dir();
    let (aiff, wav) = (dir.join("dito-e2e.aiff"), dir.join("dito-e2e.wav"));
    assert!(Command::new("say").args(["-v", "Kyoko", "-o"]).arg(&aiff).arg(text).status().expect("say").success());
    assert!(
        Command::new("afconvert")
            .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
            .arg(&aiff)
            .arg(&wav)
            .status()
            .expect("afconvert")
            .success()
    );
    AudioClip::from_wav(&std::fs::read(wav).expect("wav")).expect("16-bit mono wav")
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| "/tmp/dito-models".into()));
    let llm_id = args.next().unwrap_or_else(|| "qwen3.5-2b-instruct".into());

    let cfg = Arc::new(ConfigStore::open(dir.join("config.toml")).unwrap());
    cfg.update(|c| c.models_dir = Some(dir.join("models"))).unwrap();
    let mgr = ModelManager::new(cfg.clone());
    ensure(&mgr, "sensevoice-small").await;
    ensure(&mgr, &llm_id).await;
    let engines = LocalEngines::new(mgr, Arc::new(Runtimes));

    let spoken = "えーと、明日の3時に会議、いや、4時に会議がありますので、資料を事前に共有しておいてください。";
    let clip = synthesize(spoken);
    println!("synthesized {} ms of speech", clip.duration_ms());

    let started = Instant::now();
    let asr = engines.transcriber("sensevoice-small").await.expect("load SenseVoice");
    println!("SenseVoice loaded in {:?}", started.elapsed());
    let started = Instant::now();
    let heard = asr.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await.expect("transcribe");
    println!("recognized in {:?}: {:?} (language {:?})", started.elapsed(), heard.text, heard.language);

    let started = Instant::now();
    let llm = engines.polisher(&llm_id).await.expect("load LLM");
    println!("{llm_id} loaded in {:?}", started.elapsed());
    let prompts = Prompts::default();
    let system = build_system_prompt(&PromptInput {
        prompts: &prompts,
        style: Style::Slack,
        text: &heard.text,
        dictionary: &[],
    });
    let started = Instant::now();
    let polished = llm.polish(&PolishRequest { system: &system, text: &heard.text }).await.expect("polish");
    println!("polished in {:?}: {polished:?}", started.elapsed());
}
