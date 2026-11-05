//! Download manager against the mock file server: progress, resume, pause, checksum, space, move.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::ConfigStore;
use dito_local::catalog::{FileSpec, ModelKind, ModelSpec, Runtime};
use dito_local::{ModelManager, ModelState};
use dito_testkit::MockServer;
use sha2::{Digest, Sha256};

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8).collect()
}

fn spec(id: &str, url: &str, data: &[u8], good_hash: bool) -> ModelSpec {
    let hash = if good_hash { hex::encode(Sha256::digest(data)) } else { "0".repeat(64) };
    ModelSpec {
        id: id.into(),
        name: id.into(),
        kind: ModelKind::Asr,
        files: vec![FileSpec {
            name: "m.bin".into(),
            url: format!("{url}/files/{id}"),
            size: data.len() as u64,
            sha256: Some(hash),
        }],
        memory_mb: 1,
        runtime: Runtime::SherpaOnnx,
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

async fn until(mgr: &ModelManager, id: &str, pred: impl Fn(&ModelState) -> bool) -> ModelState {
    let mut rx = mgr.subscribe();
    tokio::time::timeout(Duration::from_secs(20), async {
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

/// The UI calls `start_download` from a non-tokio thread. The manager must spawn the job on the
/// handle captured at construction (as `AppState::build` enters its runtime), not on an ambient
/// runtime — there is none, and `tokio::spawn` would panic.
#[test]
fn start_download_works_without_an_ambient_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Arc::new(ConfigStore::open(dir.path().join("config.toml")).unwrap());
    cfg.update(|c| c.models_dir = Some(dir.path().join("models"))).unwrap();
    let dead = spec("dead", "http://127.0.0.1:9", b"x", true);

    let mgr = {
        // Built inside a live runtime, exactly like the app's startup; keep it running.
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let mgr = ModelManager::with_catalog(vec![dead], cfg.clone());
        std::mem::forget(rt); // the job must outlive this block; the test process exits anyway
        mgr
    };

    // This thread has no runtime context (like the gpui main thread).
    mgr.start_download("dead").unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while mgr.state("dead") == ModelState::NotInstalled {
        assert!(std::time::Instant::now() < deadline, "job never ran; state {:?}", mgr.state("dead"));
        std::thread::sleep(Duration::from_millis(50));
    }
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

/// Pause then cancel must discard the bytes: the old exit path keyed on the `paused` flag,
/// which `cancel` never cleared, so a cancel after a pause silently kept the file.
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
    // And the slot is free again: a fresh run completes.
    r.srv.behavior(|b| b.download_chunk_delay = Duration::ZERO);
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
}

#[tokio::test]
async fn progress_reports_speed_and_eta() {
    let r = rig(&[("a", 400_000, true)]).await;
    r.srv.behavior(|b| b.download_chunk_delay = Duration::from_millis(20));
    r.mgr.start_download("a").unwrap();
    let s = until(&r.mgr, "a", |s| matches!(s, ModelState::Downloading { bytes_per_sec, .. } if *bytes_per_sec > 1.0))
        .await;
    assert!(s.eta_secs().is_some());
    r.mgr.cancel("a");
}

#[tokio::test]
async fn delete_and_rescan_reflect_the_disk() {
    let r = rig(&[("a", 10_000, true)]).await;
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
    // A fresh manager over the same folder sees the model without any download.
    let again = ModelManager::with_catalog(r.mgr.catalog().to_vec(), r.cfg.clone());
    assert_eq!(again.state("a"), ModelState::Installed);
    r.mgr.delete("a").unwrap();
    assert_eq!(r.mgr.state("a"), ModelState::NotInstalled);
    assert!(!r.mgr.model_dir("a").exists());
}

#[tokio::test]
async fn changing_the_folder_moves_models_and_updates_config() {
    let r = rig(&[("a", 10_000, true)]).await;
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
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

/// A download paused with every byte on disk must resume into verification without a second
/// HTTP request — the expensive part (the multi-GB transfer) is already done.
#[tokio::test]
async fn resume_after_a_completed_transfer_skips_straight_to_verifying() {
    let r = rig(&[("a", 100_000, true)]).await;
    r.srv.behavior(|b| b.download_chunk_delay = Duration::from_millis(2));
    r.mgr.start_download("a").unwrap();
    until(&r.mgr, "a", |s| *s == ModelState::Installed).await;
    let hits = r.srv.requests().iter().filter(|q| q.path == "files/a").count();
    assert_eq!(hits, 1);
}

/// `fetch` announces the checksum pass as `Phase::Verifying`, and cancelling during it keeps
/// the completed `.part` file (a later call skips the transfer and only re-verifies).
#[tokio::test]
async fn fetch_reports_verifying_and_cancel_keeps_completed_bytes() {
    use dito_local::download::{Phase, Target, fetch};

    let r = rig(&[("a", 100_000, true)]).await;
    let dir = r.mgr.model_dir("a");
    let (part, dest) = (dir.join("m.bin.part"), dir.join("m.bin"));
    let url = format!("{}/files/a", r.srv.url);
    let sha = r.mgr.catalog()[0].files[0].sha256.clone().unwrap();
    let client = reqwest::Client::new();

    // Pass 1: a full fetch reports Verifying before renaming.
    let mut saw = false;
    {
        let mut on_progress = |p: Phase| saw |= matches!(p, Phase::Verifying);
        let t = Target { url: &url, part: &part, dest: &dest, size: 100_000, sha256: Some(&sha) };
        fetch(&client, &t, &tokio_util::sync::CancellationToken::new(), &mut on_progress).await.unwrap();
    }
    assert!(saw, "fetch must announce Phase::Verifying");
    assert_eq!(std::fs::metadata(&dest).unwrap().len(), 100_000);

    // Pass 2: every byte already in `.part`, token pre-cancelled — the call must stop inside
    // the checksum pass (biased select) and keep the completed bytes.
    std::fs::rename(&dest, &part).unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let mut saw2 = false;
    {
        let mut on_progress = |p: Phase| saw2 |= matches!(p, Phase::Verifying);
        let t = Target { url: &url, part: &part, dest: &dest, size: 100_000, sha256: Some(&sha) };
        let err = fetch(&client, &t, &cancel, &mut on_progress).await.unwrap_err();
        assert!(matches!(err, dito_local::download::DlError::Cancelled), "{err:?}");
    }
    assert!(saw2, "Verifying is announced even when the pass is cut short");
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 100_000, "completed bytes must survive");
    assert!(!dest.exists());
}
