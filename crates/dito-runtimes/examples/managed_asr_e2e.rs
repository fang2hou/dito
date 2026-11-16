//! Exercise the actual managed model using the shared development cache and synthetic audio only.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::audio::AudioClip;
use dito_core::config::{ConfigStore, default_data_dir};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::provider::AsrRequest;
use dito_local::catalog::NEMOTRON_TRANSCRIBE;
use dito_local::{LocalEngines, ModelManager, ModelState};
use dito_runtimes::Runtimes;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let data = PathBuf::from(args.next().expect("isolated config directory"));
    let audio = PathBuf::from(args.next().expect("synthetic audio directory"));
    let mode = args.next().unwrap_or_else(|| "run".into());
    let cfg = Arc::new(ConfigStore::open(data.join("config.toml")).unwrap());
    cfg.update(|config| {
        config.models_dir = Some(default_data_dir().join("models"));
        config.asr.local_model = NEMOTRON_TRANSCRIBE.into();
        config.polish.enabled = false;
        config.onboarded = true;
    })
    .unwrap();
    let manager = ModelManager::new(cfg);
    println!("shared model directory: {}", manager.model_dir(NEMOTRON_TRANSCRIBE).display());
    if !manager.is_installed(NEMOTRON_TRANSCRIBE) {
        manager.start_download(NEMOTRON_TRANSCRIBE).unwrap();
        let mut updates = manager.subscribe();
        tokio::time::timeout(Duration::from_secs(1800), async {
            loop {
                let state = updates.borrow_and_update().get(NEMOTRON_TRANSCRIBE).cloned();
                println!("download: {state:?}");
                match state {
                    Some(ModelState::Installed) => break,
                    Some(ModelState::Failed { message, .. }) => panic!("download failed: {message}"),
                    _ => {
                        updates.changed().await.unwrap();
                    }
                }
            }
        })
        .await
        .expect("download completed");
    }
    let engines = LocalEngines::new(manager, Arc::new(Runtimes));
    if mode == "stop-loading" {
        let loading = engines.clone();
        let task = tokio::spawn(async move { loading.warm(NEMOTRON_TRANSCRIBE).await });
        while engines.status(NEMOTRON_TRANSCRIBE) != EngineStatus::Loading {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        engines.unload(NEMOTRON_TRANSCRIBE);
        tokio::time::timeout(Duration::from_secs(10), task).await.unwrap().unwrap();
        assert_eq!(engines.status(NEMOTRON_TRANSCRIBE), EngineStatus::Stopped);
        println!("stopped during startup, no runtime published");
        return;
    }
    let start = Instant::now();
    let transcriber = engines.transcriber(NEMOTRON_TRANSCRIBE).await.expect("managed startup");
    println!("startup: {:?}, status: {:?}", start.elapsed(), engines.status(NEMOTRON_TRANSCRIBE));
    assert_eq!(engines.status(NEMOTRON_TRANSCRIBE), EngineStatus::Running);
    if mode == "hold" {
        println!("ready for abrupt-parent-exit test, owner pid: {}", std::process::id());
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
    for (name, language) in [
        ("ja_01", Some("ja")),
        ("ja_en_mix_01", Some("ja")),
        ("en_01", Some("en")),
        ("zh_01", Some("zh")),
        ("zh_en_mix_01", None),
    ] {
        let clip = AudioClip::from_wav(&std::fs::read(audio.join(format!("{name}.wav"))).unwrap()).unwrap();
        let start = Instant::now();
        let result = transcriber.transcribe(&AsrRequest { clip: &clip, language, hotwords: &[] }).await.unwrap();
        assert!(!result.text.is_empty());
        println!("{name}: {:?}, {}", start.elapsed(), result.text);
    }
    println!("worker usage: {:?}", engines.usage(NEMOTRON_TRANSCRIBE));
    let unloading = engines.clone();
    tokio::task::spawn_blocking(move || unloading.unload(NEMOTRON_TRANSCRIBE)).await.unwrap();
    assert_eq!(engines.status(NEMOTRON_TRANSCRIBE), EngineStatus::Stopped);
    let clip = AudioClip::silence(0.1);
    assert!(transcriber.transcribe(&AsrRequest { clip: &clip, language: None, hotwords: &[] }).await.is_err());
    println!("stopped, retained transcriber cannot keep the worker alive");
    assert_eq!(engines.restart(NEMOTRON_TRANSCRIBE).await, EngineStatus::Running);
    engines.unload_all();
    assert_eq!(engines.status(NEMOTRON_TRANSCRIBE), EngineStatus::Stopped);
    println!("restart and application-exit cleanup passed");
}
