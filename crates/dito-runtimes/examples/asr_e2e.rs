//! Exercise a catalog ASR through real downloads, verification, LocalEngines and transcription.
//!
//! cargo run -p dito-runtimes --no-default-features --features sherpa --example asr_e2e --
//!   /tmp/dito-haas-e2e haas-ja /path/to/bench/audio
//! Uses synthetic bench WAVs only. Does not capture or insert text into other applications.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::audio::AudioClip;
use dito_core::config::ConfigStore;
use dito_core::error::ErrorKind;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::provider::AsrRequest;
use dito_local::catalog::ModelFormat;
use dito_local::{LocalEngines, ModelManager, ModelState};
use dito_runtimes::Runtimes;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().expect("data directory"));
    let model = args.next().unwrap_or_else(|| "haas-ja".into());
    let audio = PathBuf::from(args.next().expect("bench audio directory"));
    let cfg = Arc::new(ConfigStore::open(root.join("config.toml")).unwrap());
    cfg.update(|c| {
        c.models_dir = Some(root.join("models"));
        c.asr.local_model = model.clone();
    })
    .unwrap();
    let manager = ModelManager::new(cfg);
    if !manager.is_installed(&model) {
        manager.start_download(&model).expect("start download");
        let mut updates = manager.subscribe();
        tokio::time::timeout(Duration::from_secs(1800), async {
            loop {
                let state = updates.borrow_and_update().get(&model).cloned();
                match state {
                    Some(ModelState::Installed) => break,
                    Some(ModelState::Failed { message, .. }) => panic!("download: {message}"),
                    _ => {
                        updates.changed().await.expect("download updates");
                    }
                }
            }
        })
        .await
        .expect("download and verification within thirty minutes");
    }
    println!("{model}: files downloaded and verified");
    let format = manager.spec(&model).unwrap().format;
    let detects_language = matches!(format, ModelFormat::SenseVoice { .. });
    let reports_no_language = matches!(format, ModelFormat::NemoTdt { .. });
    let engines = LocalEngines::new(manager, Arc::new(Runtimes));
    let start = Instant::now();
    let transcriber = engines.transcriber(&model).await.expect("load native runtime");
    println!("loaded in {:?}", start.elapsed());
    assert_eq!(engines.status(&model), EngineStatus::Running);
    let mut japanese = None;
    for (clip, hint) in [("ja_01", Some("ja")), ("ja_en_mix_01", None), ("en_01", Some("en")), ("zh_01", Some("zh"))] {
        let wav = std::fs::read(audio.join(format!("{clip}.wav"))).unwrap();
        let input = AudioClip::from_wav(&wav).expect("bench WAV");
        let start = Instant::now();
        let result = transcriber
            .transcribe(&AsrRequest { clip: &input, language: hint, hotwords: &[] })
            .await
            .expect("transcribe, including non-Japanese hints");
        println!("{clip}: {:?}, {:?}, {:?}", start.elapsed(), result.language, result.text);
        if detects_language {
            let expected = hint.unwrap_or("ja");
            let automatic = transcriber
                .transcribe(&AsrRequest { clip: &input, language: None, hotwords: &[] })
                .await
                .expect("automatic language detection");
            assert_eq!(automatic.language.as_deref(), Some(expected));
        } else if reports_no_language {
            assert!(result.language.is_none(), "a decoder without language detection must not invent one");
        }
        if clip == "ja_01" {
            assert!(result.text.contains("会議"));
            japanese = Some(input);
        }
    }
    let original = japanese.unwrap();
    let mut long = original.clone();
    long.samples = original.samples.iter().copied().cycle().take(35 * 16_000).collect();
    let result = transcriber
        .transcribe(&AsrRequest { clip: &long, language: None, hotwords: &[] })
        .await
        .expect("long input through chunking");
    assert!(result.text.matches("会議").count() >= 5, "chunks must survive: {:?}", result.text);
    let silence = AudioClip::silence(2.);
    let result = transcriber.transcribe(&AsrRequest { clip: &silence, language: None, hotwords: &[] }).await;
    assert!(matches!(&result, Err(e) if e.kind == ErrorKind::NoSpeech));
    drop(transcriber);
    engines.unload(&model);
    assert_eq!(engines.status(&model), EngineStatus::Stopped);
    assert_eq!(engines.restart(&model).await, EngineStatus::Running);
    engines.unload_all();
    println!("PASS: download, verification, load, language hints, long audio, silence, stop and restart");
}
