//! Download manager against the mock file server.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::ConfigStore;
use dito_local::catalog::{FileSpec, ModelFormat, ModelKind, ModelSpec};
use dito_local::download::{DlError, Phase, Target, fetch};
use dito_local::{ModelManager, ModelState};
use dito_testkit::MockServer;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const HANG_GUARD: Duration = Duration::from_secs(60);

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8).collect()
}

fn spec(id: &str, url: &str, data: &[u8], good_hash: bool) -> ModelSpec {
    let hash = if good_hash { hex::encode(Sha256::digest(data)) } else { "0".repeat(64) };
    ModelSpec {
        id: id.into(),
        name: id.into(),
        kind: ModelKind::Asr,
        text_capabilities: None,
        files: vec![FileSpec {
            name: "m.bin".into(),
            url: format!("{url}/files/{id}"),
            size: data.len() as u64,
            sha256: Some(hash),
        }],
        parameters_m: 1,
        quantization: dito_local::catalog::Quantization::Q8_0,
        memory_mb: 1,
        format: ModelFormat::SenseVoice { model: "f", tokens: "f" },
        language_focus: None,
    }
}

struct Rig {
    srv: MockServer,
    mgr: ModelManager,
    cfg: Arc<ConfigStore>,
    dir: tempfile::TempDir,
}

async fn rig(models: &[(&str, usize, bool)]) -> Rig {
    let srv = MockServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cfg = Arc::new(ConfigStore::open(dir.path().join("config.toml")).unwrap());
    cfg.update(|c| c.models_dir = Some(dir.path().join("models"))).unwrap();
    let mut catalog = Vec::new();
    for (id, size, good) in models {
        let data = payload(*size);
        srv.add_file(id, data.clone());
        catalog.push(spec(id, &srv.url, &data, *good));
    }
    let mgr = ModelManager::with_catalog(catalog, cfg.clone());
    Rig { srv, mgr, cfg, dir }
}

/// The timeout only turns a hang into a failure.
async fn until(mgr: &ModelManager, id: &str, pred: impl Fn(&ModelState) -> bool) -> ModelState {
    let mut rx = mgr.subscribe();
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let s = rx.borrow_and_update().get(id).cloned().unwrap_or(ModelState::NotInstalled);
            if pred(&s) {
                return s;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out; state now {:?}", mgr.state(id)))
}

/// The UI calls `start_download` from a thread with no tokio runtime; the manager must spawn on the handle it
/// captured at construction.
#[test]
fn start_download_works_without_an_ambient_runtime() {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let r = rt.block_on(rig(&[("a", 10_000, true)]));
    r.mgr.start_download("a").unwrap();
    rt.block_on(until(&r.mgr, "a", |s| *s == ModelState::Installed));
}

#[tokio::test]
async fn downloads_verifies_and_marks_installed() {
    let r = rig(&[("a", 200_000, true)]).await;
    assert_eq!(r.mgr.state("a"), ModelState::NotInstalled);
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
    let file = r.mgr.model_dir("a").join("m.bin");
    assert_eq!(std::fs::read(&file).unwrap(), payload(200_000));
    assert!(!r.mgr.model_dir("a").join("m.bin.part").exists());
    assert_eq!(r.mgr.storage().used_bytes, 200_000);
}

#[tokio::test]
async fn a_cut_connection_fails_then_resumes_from_the_partial_bytes() {
    let r = rig(&[("a", 300_000, true)]).await;
    r.srv.behavior(|b| b.cut_download_after = Some(100_000));
    r.mgr.start_download("a").unwrap();
    let failed = until(&r.mgr, "a", |s| matches!(s, ModelState::Failed { .. })).await;
    let ModelState::Failed { done, total, .. } = failed else { unreachable!() };
    assert_eq!(total, 300_000);
    assert!((100_000 - 16 * 1024..=100_000).contains(&done), "partial bytes kept: {done}");

    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
    let ranged = r
        .srv
        .requests()
        .iter()
        .filter(|q| q.path == "files/a")
        .any(|q| q.headers.iter().any(|(k, v)| k == "range" && v.starts_with("bytes=") && v != "bytes=0-"));
    assert!(ranged, "second attempt must send a Range header");
    assert_eq!(std::fs::read(r.mgr.model_dir("a").join("m.bin")).unwrap(), payload(300_000));
}

#[tokio::test]
async fn checksum_mismatch_discards_the_file_and_reports_failure() {
    let r = rig(&[("bad", 50_000, false)]).await;
    r.mgr.start_download("bad").unwrap();
    let s = until(&r.mgr, "bad", |s| matches!(s, ModelState::Failed { .. })).await;
    let ModelState::Failed { message, .. } = s else { unreachable!() };
    assert!(message.contains("checksum"), "{message}");
    assert!(!r.mgr.model_dir("bad").join("m.bin").exists());
    assert!(!r.mgr.model_dir("bad").join("m.bin.part").exists());
}

#[tokio::test]
async fn pause_keeps_partial_data_and_resume_finishes() {
    let r = rig(&[("a", 400_000, true)]).await;
    r.srv.behavior(|b| b.download_chunk_delay = Duration::from_millis(20));
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| matches!(s, ModelState::Downloading { done, .. } if *done > 0)).await;
    r.mgr.pause("a");
    let s = until(&r.mgr, "a", |s| matches!(s, ModelState::Paused { .. })).await;
    let ModelState::Paused { done, total } = s else { unreachable!() };
    assert!(done > 0 && done < total, "{done}/{total}");
    assert!(r.mgr.model_dir("a").join("m.bin.part").exists());

    r.srv.behavior(|b| b.download_chunk_delay = Duration::ZERO);
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
}

#[tokio::test]
async fn cancel_removes_partial_data() {
    let r = rig(&[("a", 400_000, true)]).await;
    r.srv.behavior(|b| b.download_chunk_delay = Duration::from_millis(20));
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| matches!(s, ModelState::Downloading { done, .. } if *done > 0)).await;
    r.mgr.cancel("a");
    until(&r.mgr, "a", |s| *s == ModelState::NotInstalled).await;
    assert!(!r.mgr.model_dir("a").join("m.bin.part").exists());
}

/// Pause then cancel must discard the bytes: `cancel` has to clear the `paused` flag, or the exit path keeps the file
/// as if the download were only paused.
#[tokio::test]
async fn cancel_after_pause_discards_the_partial_file() {
    let r = rig(&[("a", 400_000, true)]).await;
    r.srv.behavior(|b| b.download_chunk_delay = Duration::from_millis(20));
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| matches!(s, ModelState::Downloading { done, .. } if *done > 0)).await;
    r.mgr.pause("a");
    until(&r.mgr, "a", |s| matches!(s, ModelState::Paused { .. })).await;
    r.mgr.cancel("a");
    until(&r.mgr, "a", |s| *s == ModelState::NotInstalled).await;
    assert!(!r.mgr.model_dir("a").join("m.bin.part").exists());
    r.srv.behavior(|b| b.download_chunk_delay = Duration::ZERO);
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
}

fn install_on_disk(mgr: &ModelManager, id: &str, size: usize) {
    std::fs::create_dir_all(mgr.model_dir(id)).unwrap();
    std::fs::write(mgr.model_dir(id).join("m.bin"), payload(size)).unwrap();
    mgr.refresh();
}

#[tokio::test]
async fn delete_removes_the_model_and_a_fresh_manager_sees_the_disk() {
    let r = rig(&[("a", 10_000, true)]).await;
    install_on_disk(&r.mgr, "a", 10_000);
    let again = ModelManager::with_catalog(r.mgr.catalog().to_vec(), r.cfg.clone());
    assert_eq!(again.state("a"), ModelState::Installed);
    r.mgr.delete("a").unwrap();
    assert_eq!(r.mgr.state("a"), ModelState::NotInstalled);
    assert!(!r.mgr.model_dir("a").exists());
}

#[tokio::test]
async fn changing_the_folder_moves_models_and_updates_config() {
    let r = rig(&[("a", 10_000, true)]).await;
    install_on_disk(&r.mgr, "a", 10_000);
    let new_dir = r.dir.path().join("elsewhere");
    r.mgr.change_dir(&new_dir).unwrap();
    assert_eq!(r.cfg.get().models_dir.as_deref(), Some(new_dir.as_path()));
    assert!(new_dir.join("a").join("m.bin").exists());
    assert_eq!(r.mgr.state("a"), ModelState::Installed);
}

#[tokio::test]
async fn missing_file_on_the_server_fails_with_http_404() {
    let r = rig(&[("a", 1_000, true)]).await;
    let mut cat = r.mgr.catalog().to_vec();
    cat[0].files[0].url = format!("{}/files/nope", r.srv.url);
    let mgr = ModelManager::with_catalog(cat, r.cfg.clone());
    mgr.start_download("a").unwrap();
    let s = until(&mgr, "a", |s| matches!(s, ModelState::Failed { .. })).await;
    let ModelState::Failed { message, .. } = s else { unreachable!() };
    assert!(message.contains("404"), "{message}");
}

struct FetchFixture {
    _rig: Rig,
    url: String,
    sha: String,
    part: PathBuf,
    dest: PathBuf,
}

impl FetchFixture {
    async fn new() -> Self {
        let r = rig(&[("a", 100_000, true)]).await;
        let dir = r.mgr.model_dir("a");
        std::fs::create_dir_all(&dir).unwrap();
        let file = &r.mgr.catalog()[0].files[0];
        Self {
            url: file.url.clone(),
            sha: file.sha256.clone().unwrap(),
            part: dir.join("m.bin.part"),
            dest: dir.join("m.bin"),
            _rig: r,
        }
    }

    fn target(&self) -> Target<'_> {
        Target { url: &self.url, part: &self.part, dest: &self.dest, size: 100_000, sha256: Some(&self.sha) }
    }
}

#[tokio::test]
async fn fetch_announces_verifying_before_the_rename() {
    let fx = FetchFixture::new().await;
    let mut saw_verifying = false;
    let mut on_progress = |p: Phase| saw_verifying |= matches!(p, Phase::Verifying);
    fetch(&reqwest::Client::new(), &fx.target(), &CancellationToken::new(), &mut on_progress).await.unwrap();
    assert!(saw_verifying);
    assert_eq!(std::fs::metadata(&fx.dest).unwrap().len(), 100_000);
}

#[tokio::test]
async fn cancelling_during_verification_keeps_the_completed_bytes() {
    let fx = FetchFixture::new().await;
    std::fs::write(&fx.part, payload(100_000)).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut saw_verifying = false;
    let mut on_progress = |p: Phase| saw_verifying |= matches!(p, Phase::Verifying);
    let err = fetch(&reqwest::Client::new(), &fx.target(), &cancel, &mut on_progress).await.unwrap_err();
    assert!(matches!(err, DlError::Cancelled), "{err:?}");
    assert!(saw_verifying);
    assert_eq!(std::fs::metadata(&fx.part).unwrap().len(), 100_000);
    assert!(!fx.dest.exists());
}
