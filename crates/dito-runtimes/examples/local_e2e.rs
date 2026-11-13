//! Real on-device check of SenseVoice and a Qwen3 model, with speech synthesized by macOS `say`.
//!
//!   cargo run --release -p dito-runtimes --example local_e2e -- /tmp/dito-models qwen3.5-2b-instruct
//!
//! Needs network access for the first download (models are cached in the given directory).

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::audio::AudioClip;
use dito_core::config::{ConfigStore, LocalTextParams};
use dito_core::error::ErrorKind;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Lang;
use dito_core::prompts::{AppTarget, PromptInput, Prompts, build_prompt};
use dito_core::provider::{AsrRequest, TextRequest};
use dito_local::{LocalEngines, ModelManager, ModelState};
use dito_runtimes::Runtimes;

const ASR: &str = "sensevoice-small";

async fn ensure(mgr: &ModelManager, id: &str) {
    if mgr.is_installed(id) {
        return;
    }
    println!("downloading {id} ...");
    mgr.start_download(id).expect("start download");
    let mut rx = mgr.subscribe();
    let mut last_report = Instant::now();
    loop {
        match rx.borrow_and_update().get(id).cloned().unwrap_or(ModelState::NotInstalled) {
            ModelState::Installed => return,
            ModelState::Failed { message, .. } => panic!("download of {id} failed: {message}"),
            ModelState::Downloading { done, total, bytes_per_sec }
                if last_report.elapsed() > Duration::from_secs(5) =>
            {
                last_report = Instant::now();
                println!("  {id}: {:.0}% ({:.1} MB/s)", done as f64 * 100.0 / total as f64, bytes_per_sec / 1e6);
            }
            _ => {}
        }
        let _ = tokio::time::timeout(Duration::from_secs(30), rx.changed()).await;
    }
}

fn synthesize(voice: &str, text: &str) -> AudioClip {
    let dir = std::env::temp_dir();
    let (aiff, wav) = (dir.join("dito-e2e.aiff"), dir.join("dito-e2e.wav"));
    assert!(Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).status().expect("say").success());
    let convert = Command::new("afconvert")
        .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
        .arg(&aiff)
        .arg(&wav)
        .status()
        .expect("afconvert");
    assert!(convert.success());
    AudioClip::from_wav(&std::fs::read(wav).expect("wav")).expect("16-bit mono wav")
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| "/tmp/dito-models".into()));
    let polish_id = args.next().unwrap_or_else(|| "qwen3.5-2b-instruct".into());

    let cfg = Arc::new(ConfigStore::open(dir.join("config.toml")).unwrap());
    cfg.update(|c| c.models_dir = Some(dir.join("models"))).unwrap();
    let mgr = ModelManager::new(cfg.clone());
    ensure(&mgr, ASR).await;
    ensure(&mgr, &polish_id).await;
    let engines = LocalEngines::new(mgr.clone(), Arc::new(Runtimes));

    let started = Instant::now();
    let asr = engines.transcriber(ASR).await.expect("load SenseVoice");
    println!("SenseVoice loaded in {:?}", started.elapsed());
    assert_eq!(engines.status(ASR), EngineStatus::Running);

    let cases = [
        ("Kyoko", "明日の午後三時に会議があります。資料を事前に共有してください。", "ja", "会議"),
        ("Samantha", "Please send me the meeting notes before Friday afternoon.", "en", "meeting"),
        ("Tingting", "请把周五的会议纪要发给我，谢谢。", "zh", "会议"),
    ];
    for (voice, text, lang, needle) in cases {
        let clip = synthesize(voice, text);
        let started = Instant::now();
        let heard =
            asr.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await.expect("transcribe");
        println!("[{lang}] {text:?} -> {:?} ({:?}, {:?})", heard.text, heard.language, started.elapsed());
        assert!(heard.text.to_lowercase().contains(needle), "[{lang}] expected {needle:?} in {:?}", heard.text);
    }

    let silence = AudioClip::silence(2.);
    let heard = asr.transcribe(&AsrRequest { clip: &silence, language: None, hotwords: &[] }).await;
    assert!(matches!(&heard, Err(e) if e.kind == ErrorKind::NoSpeech), "silence must report no speech, got {heard:?}");

    let transcript = "えーと 明日の3時に会議、いや4時に会議があるので、資料を、あの、事前に共有しておいてください";
    let prompts = Prompts::default();
    let prompt = build_prompt(&PromptInput {
        prompts: &prompts,
        target: AppTarget::General,
        lang: Lang::Ja,
        text: transcript,
        wordbook: &["Acme".into()],
    });
    let started = Instant::now();
    let llm = engines.text_model(&polish_id, &LocalTextParams::default()).await.expect("load text model");
    println!("{polish_id} loaded in {:?}", started.elapsed());
    let started = Instant::now();
    let polished = llm
        .polish(&TextRequest { system: &prompt.system, examples: prompt.examples, text: &prompt.user })
        .await
        .expect("polish");
    println!("polished in {:?}: {polished:?}", started.elapsed());
    assert!(!polished.contains("えーと") && !polished.contains("あの"), "fillers removed: {polished}");

    engines.unload(ASR);
    let tokens = mgr.model_dir(ASR).join("tokens.txt");
    let original = std::fs::read(&tokens).unwrap();
    std::fs::write(&tokens, b"broken").unwrap();
    mgr.refresh();
    let status = engines.restart(ASR).await;
    assert!(!matches!(status, EngineStatus::Running), "must not claim to run with a broken model: {status:?}");
    std::fs::write(&tokens, original).unwrap();
    mgr.refresh();
    assert_eq!(engines.restart(ASR).await, EngineStatus::Running);

    println!("ALL STEPS PASSED");
}
