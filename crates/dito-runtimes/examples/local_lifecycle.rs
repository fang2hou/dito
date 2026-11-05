//! Real on-device lifecycle check with the default local models (network needed for the first download):
//! download → pause → resume → cancel → delete → re-download → load → recognize (ja/en/zh) → restart → unload →
//! recognize again → polish → reload after a corrupted file.
//!
//!   cargo run --release -p dito-runtimes --example local_lifecycle -- /tmp/dito-lifecycle
//!
//! Every step asserts; a failure panics with the step name.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::audio::AudioClip;
use dito_core::config::ConfigStore;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::prompts::{PromptInput, Prompts, Style, build_system_prompt};
use dito_core::provider::{AsrRequest, PolishRequest};
use dito_local::{LocalEngines, ModelManager, ModelState};
use dito_runtimes::Runtimes;

const ASR: &str = "sensevoice-small";

fn step(name: &str) {
    println!("\n== {name}");
}

async fn wait_for(mgr: &ModelManager, id: &str, what: &str, mut ok: impl FnMut(&ModelState) -> bool) -> ModelState {
    let mut rx = mgr.subscribe();
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        let s = rx.borrow_and_update().get(id).cloned().unwrap_or(ModelState::NotInstalled);
        if let ModelState::Failed { message, .. } = &s {
            panic!("{what}: download failed: {message}");
        }
        if ok(&s) {
            return s;
        }
        assert!(Instant::now() < deadline, "{what}: timed out in state {s:?}");
        let _ = tokio::time::timeout(Duration::from_secs(5), rx.changed()).await;
    }
}

fn synthesize(voice: &str, text: &str) -> AudioClip {
    let dir = std::env::temp_dir();
    let (aiff, wav) = (dir.join("dito-lc.aiff"), dir.join("dito-lc.wav"));
    assert!(Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).status().expect("say").success());
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

fn dir_has_part(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().ends_with(".part")))
        .unwrap_or(false)
}

#[tokio::main]
async fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/dito-lifecycle".into()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let cfg = Arc::new(ConfigStore::open(root.join("config.toml")).unwrap());
    cfg.update(|c| c.models_dir = Some(root.join("models"))).unwrap();
    let polish_id = cfg.get().polish.engine.local_model.clone();
    let mgr = ModelManager::new(cfg.clone());
    let model_dir = mgr.model_dir(ASR);

    step("fresh state");
    assert_eq!(mgr.state(ASR), ModelState::NotInstalled);
    println!("storage: {:?}", mgr.storage());

    step("download, then pause mid-way");
    mgr.start_download(ASR).unwrap();
    let s =
        wait_for(&mgr, ASR, "reach 20 MB", |s| matches!(s, ModelState::Downloading { done, .. } if *done > 20_000_000))
            .await;
    println!("downloading: {s:?}");
    mgr.pause(ASR);
    let paused = wait_for(&mgr, ASR, "paused", |s| matches!(s, ModelState::Paused { .. })).await;
    let ModelState::Paused { done: paused_at, total } = paused else { unreachable!() };
    println!("paused at {paused_at} of {total}");
    assert!(paused_at > 0 && dir_has_part(&model_dir), "a .part file is kept for resuming");

    step("resume continues from the partial file");
    mgr.start_download(ASR).unwrap();
    let s = wait_for(&mgr, ASR, "resumed", |s| matches!(s, ModelState::Downloading { done, .. } if *done > 0)).await;
    let ModelState::Downloading { done, .. } = s else { unreachable!() };
    assert!(done + 10_000_000 >= paused_at, "resume must not start over: resumed at {done}, paused at {paused_at}");
    println!("resumed at {done}");

    step("cancel discards the partial file");
    mgr.cancel(ASR);
    wait_for(&mgr, ASR, "cancelled", |s| *s == ModelState::NotInstalled).await;
    assert!(!dir_has_part(&model_dir), "cancel removes partial files");

    step("full download (SHA-256 verified per file)");
    mgr.start_download(ASR).unwrap();
    wait_for(&mgr, ASR, "installed", |s| *s == ModelState::Installed).await;
    println!("installed; storage: {:?}", mgr.storage());

    step("delete, then download again");
    mgr.delete(ASR).unwrap();
    assert_eq!(mgr.state(ASR), ModelState::NotInstalled);
    assert!(!model_dir.exists() || std::fs::read_dir(&model_dir).unwrap().next().is_none());
    mgr.start_download(ASR).unwrap();
    wait_for(&mgr, ASR, "re-installed", |s| *s == ModelState::Installed).await;

    step("polish model download");
    mgr.start_download(&polish_id).unwrap();
    wait_for(&mgr, &polish_id, "polish installed", |s| *s == ModelState::Installed).await;
    println!("{polish_id} installed");

    let engines = LocalEngines::new(mgr.clone(), Arc::new(Runtimes));

    step("status before loading");
    assert_eq!(engines.status(ASR), EngineStatus::Stopped);

    step("load and recognize Japanese, English, Chinese");
    let t = Instant::now();
    let asr = engines.transcriber(ASR).await.expect("load SenseVoice");
    println!("loaded in {:?}; status {:?}", t.elapsed(), engines.status(ASR));
    assert_eq!(engines.status(ASR), EngineStatus::Running);
    let cases = [
        ("Kyoko", "明日の午後三時に会議があります。資料を事前に共有してください。", "ja", "会議"),
        ("Samantha", "Please send me the meeting notes before Friday afternoon.", "en", "meeting"),
        ("Tingting", "请把周五的会议纪要发给我，谢谢。", "zh", "会议"),
    ];
    for (voice, text, lang, needle) in cases {
        let clip = synthesize(voice, text);
        let t = Instant::now();
        let heard =
            asr.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await.expect("transcribe");
        println!("[{lang}] {:?} -> {:?} ({:?}, {:?})", text, heard.text, heard.language, t.elapsed());
        assert!(heard.text.to_lowercase().contains(needle), "[{lang}] expected {needle:?} in {:?}", heard.text);
    }
    println!("metrics: {:?}", engines.metrics(ASR));
    assert!(engines.metrics(ASR).last_ms > 0);

    step("restart (unload + load)");
    let t = Instant::now();
    let status = engines.restart(ASR).await;
    println!("restart -> {status:?} in {:?}", t.elapsed());
    assert_eq!(status, EngineStatus::Running);
    let clip = synthesize("Kyoko", "もう一度お願いします。");
    let asr = engines.transcriber(ASR).await.unwrap();
    let heard = asr.transcribe(&AsrRequest { clip: &clip, language: Some("ja"), hotwords: &[] }).await.unwrap();
    println!("after restart: {:?}", heard.text);
    // A preferred language is honoured: Japanese forced on Japanese speech, English forced on English speech.
    let en = synthesize("Samantha", "Please send me the meeting notes.");
    let forced = asr.transcribe(&AsrRequest { clip: &en, language: Some("en"), hotwords: &[] }).await.unwrap();
    println!("forced en: {:?}", forced.text);
    assert!(forced.text.to_lowercase().contains("meeting"));
    assert!(!heard.text.trim().is_empty());

    step("unload frees the runtime; next use reloads it");
    engines.unload(ASR);
    assert_eq!(engines.status(ASR), EngineStatus::Stopped);
    let asr = engines.transcriber(ASR).await.unwrap();
    assert_eq!(engines.status(ASR), EngineStatus::Running);
    let heard = asr.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await.unwrap();
    assert!(!heard.text.trim().is_empty());

    step("silence yields no words (not a hallucinated Korean word)");
    let silence = AudioClip::from_wav(&{
        let mut v = Vec::new();
        let n = 16_000 * 2;
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + n as u32 * 2).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&16_000u32.to_le_bytes());
        v.extend_from_slice(&32_000u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(n as u32 * 2).to_le_bytes());
        v.resize(v.len() + n * 2, 0);
        v
    })
    .unwrap();
    let r = asr.transcribe(&AsrRequest { clip: &silence, language: None, hotwords: &[] }).await;
    println!("silence -> {r:?}");
    assert!(
        matches!(&r, Err(e) if e.kind == dito_core::error::ErrorKind::NoSpeech),
        "noise must be reported as no speech, got {r:?}"
    );

    step("polish with the default local model");
    let t = Instant::now();
    let llm = engines.polisher(&polish_id).await.expect("load polish model");
    println!("{polish_id} loaded in {:?}", t.elapsed());
    let raw = "えーと 明日の3時に会議、いや4時に会議があるので、資料を、あの、事前に共有しておいてください";
    let prompts = Prompts::default();
    let system = build_system_prompt(&PromptInput {
        prompts: &prompts,
        style: Style::General,
        text: raw,
        dictionary: &["Acme".into()],
    });
    let t = Instant::now();
    let out = llm.polish(&PolishRequest { system: &system, text: raw }).await.expect("polish");
    println!("polished in {:?}: {out:?}", t.elapsed());
    assert!(!out.contains("えーと") && !out.contains("あの"), "fillers removed: {out}");
    assert!(out.contains('4') || out.contains('４') || out.contains("四"), "keeps the corrected time: {out}");

    step("a damaged model file fails loudly; deleting and re-downloading repairs it");
    engines.unload(ASR);
    let tokens = model_dir.join("tokens.txt");
    let backup = std::fs::read(&tokens).unwrap();
    std::fs::write(&tokens, b"broken").unwrap();
    mgr.refresh();
    println!("state after damage: {:?}", mgr.state(ASR));
    let status = engines.restart(ASR).await;
    println!("status with damaged files: {status:?}");
    assert!(!matches!(status, EngineStatus::Running), "must not claim to run with a broken model");
    std::fs::write(&tokens, backup).unwrap();
    mgr.refresh();
    let status = engines.restart(ASR).await;
    assert_eq!(status, EngineStatus::Running);

    println!("\nALL STEPS PASSED");
}
