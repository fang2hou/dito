//! Local model lifecycle on disk: scan, download (pause, resume, cancel), delete, move.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dito_core::config::{ConfigStore, default_data_dir};
use dito_core::trace_log;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::catalog::{ModelSpec, default_catalog};
use crate::disk::{copy_dir, dir_size, free_space};
use crate::download::{self, DlError};
use crate::manager_error::ManagerError;
use crate::manifest::{self, Manifest, ManifestError};

#[derive(Debug, Clone, PartialEq)]
pub enum ModelState {
    NotInstalled,
    Downloading {
        done: u64,
        total: u64,
        bytes_per_sec: f64,
    },
    /// A cancel here keeps the completed `.part` file.
    Verifying {
        done: u64,
        total: u64,
    },
    Paused {
        done: u64,
        total: u64,
    },
    Failed {
        message: String,
        done: u64,
        total: u64,
    },
    Installed,
}

impl ModelState {
    pub fn eta_secs(&self) -> Option<u64> {
        match self {
            Self::Downloading { done, total, bytes_per_sec } if *bytes_per_sec > 1.0 => {
                Some((total.saturating_sub(*done) as f64 / bytes_per_sec).ceil() as u64)
            }
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Downloading { .. } | Self::Verifying { .. })
    }
}

pub type Snapshot = Arc<BTreeMap<String, ModelState>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Storage {
    pub dir: PathBuf,
    pub used_bytes: u64,
}

struct Job {
    cancel: CancellationToken,
    paused: Arc<AtomicBool>,
}

/// Removes the job entry when the download task finishes, including on panic: a leftover entry would make
/// pause and cancel hit a dead job forever.
struct JobGuard {
    inner: Arc<Inner>,
    spec: ModelSpec,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.inner.jobs.lock().expect("manager mutex poisoned").remove(&self.spec.id);
        // A task that died mid-flight leaves an active state; rescanning shows a resumable row instead of a dead one.
        let mgr = ModelManager { inner: self.inner.clone() };
        if mgr.state(&self.spec.id).is_active() {
            mgr.set(&self.spec.id, mgr.scan(&self.spec));
        }
    }
}

/// Marks the models folder as being moved until dropped, so a failed move releases it too.
struct MoveGuard<'a>(&'a AtomicBool);

impl Drop for MoveGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct Inner {
    catalog: Vec<ModelSpec>,
    config: Arc<ConfigStore>,
    tx: watch::Sender<Snapshot>,
    jobs: Mutex<HashMap<String, Job>>,
    /// Set and checked only while `jobs` is locked, so a move and a download can never both start.
    moving: AtomicBool,
    client: reqwest::Client,
    manifest_url: Option<String>,
    manifest_cache: tokio::sync::Mutex<Option<(Instant, Manifest)>>,
    /// Captured when the manager is built inside a runtime (the app enters its runtime around
    /// construction). `start_download` is called from UI threads, where there is no ambient
    /// runtime, so it spawns on this handle instead of `tokio::spawn`.
    rt: Option<tokio::runtime::Handle>,
}

#[derive(Clone)]
pub struct ModelManager {
    inner: Arc<Inner>,
}

const FREE_MARGIN: u64 = 200 * 1024 * 1024;
const MANIFEST_TTL: Duration = Duration::from_secs(5 * 60);

/// Reports the percent, in steps of ten, each time a download first passes one after `start` bytes.
fn same_volume(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    matches!((std::fs::metadata(a), std::fs::metadata(b)), (Ok(a), Ok(b)) if a.dev() == b.dev())
}

/// Renames each model folder, and on the first failure renames the ones already moved back.
fn rename_all(moving: &[(PathBuf, PathBuf)]) -> Result<(), ManagerError> {
    for (done, (from, to)) in moving.iter().enumerate() {
        if let Err(e) = std::fs::rename(from, to) {
            undo_move(&moving[..done]);
            return Err(e.into());
        }
    }
    Ok(())
}

/// Copies every model folder into a staging folder next to its target, after checking the space for all of them,
/// then renames the copies into place. On failure only what this move created is removed: the staging folders and
/// the targets, which did not exist before. The originals are never touched.
fn copy_all(moving: &[(PathBuf, PathBuf)], new_dir: &Path) -> Result<(), ManagerError> {
    let needed = moving.iter().map(|(from, _)| dir_size(from)).sum::<u64>();
    let free = free_space(new_dir);
    if needed > free {
        return Err(ManagerError::InsufficientSpace { needed, free });
    }
    let staging: Vec<PathBuf> = moving.iter().map(|(_, to)| staging_for(to)).collect();
    if let Some(taken) = staging.iter().find(|path| path.symlink_metadata().is_ok()) {
        return Err(ManagerError::TargetExists(taken.clone()));
    }
    let remove_all = |paths: &[PathBuf]| {
        for path in paths {
            if path.exists()
                && let Err(e) = std::fs::remove_dir_all(path)
            {
                tracing::warn!("could not remove {} after a failed move: {e}", path.display());
            }
        }
    };
    for (done, ((from, _), stage)) in moving.iter().zip(&staging).enumerate() {
        if let Err(e) = copy_dir(from, stage) {
            remove_all(&staging[..=done]);
            return Err(e.into());
        }
    }
    for (done, ((_, to), stage)) in moving.iter().zip(&staging).enumerate() {
        if let Err(e) = std::fs::rename(stage, to) {
            let placed: Vec<PathBuf> = moving[..done].iter().map(|(_, to)| to.clone()).collect();
            remove_all(&placed);
            remove_all(&staging[done..]);
            return Err(e.into());
        }
    }
    Ok(())
}

fn staging_for(target: &Path) -> PathBuf {
    let name = target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    target.with_file_name(format!(".{name}.moving-{}", std::process::id()))
}

/// Puts moved folders back where they were: a renamed one is renamed back, a copied one is removed, since its
/// original is still in place.
fn undo_move(moved: &[(PathBuf, PathBuf)]) {
    for (from, to) in moved {
        let undone = if from.exists() { std::fs::remove_dir_all(to) } else { std::fs::rename(to, from) };
        if let Err(e) = undone {
            tracing::warn!("could not undo the move of {}: {e}", to.display());
        }
    }
}

fn milestone_reached(start: u64, total: u64) -> impl FnMut(u64) -> Option<u64> {
    let step = move |done: u64| (done * 100).checked_div(total).map_or(0, |percent| percent / 10 * 10);
    let mut last = step(start);
    move |done| {
        let now = step(done);
        (now > last).then(|| {
            last = now;
            now
        })
    }
}

impl ModelManager {
    pub fn new(config: Arc<ConfigStore>) -> Self {
        Self::with_manifest_url(default_catalog(), config, manifest::default_manifest_url())
    }

    /// Uses only the supplied catalog URLs, without fetching a remote manifest.
    pub fn with_catalog(catalog: Vec<ModelSpec>, config: Arc<ConfigStore>) -> Self {
        Self::with_manifest_url(catalog, config, None)
    }

    pub fn with_manifest_url(catalog: Vec<ModelSpec>, config: Arc<ConfigStore>, manifest_url: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("dito/0.1")
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        let (tx, _) = watch::channel(Arc::new(BTreeMap::new()));
        let m = Self {
            inner: Arc::new(Inner {
                catalog,
                config,
                tx,
                jobs: Mutex::default(),
                moving: AtomicBool::new(false),
                client,
                manifest_url,
                manifest_cache: tokio::sync::Mutex::new(None),
                rt: tokio::runtime::Handle::try_current().ok(),
            }),
        };
        m.refresh();
        m
    }

    pub fn catalog(&self) -> &[ModelSpec] {
        &self.inner.catalog
    }

    pub fn spec(&self, id: &str) -> Option<&ModelSpec> {
        self.inner.catalog.iter().find(|m| m.id == id)
    }

    pub fn dir(&self) -> PathBuf {
        self.inner.config.get().models_dir.clone().unwrap_or_else(|| default_data_dir().join("models"))
    }

    pub fn model_dir(&self, id: &str) -> PathBuf {
        self.dir().join(id)
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.inner.tx.subscribe()
    }

    pub fn state(&self, id: &str) -> ModelState {
        self.inner.tx.borrow().get(id).cloned().unwrap_or(ModelState::NotInstalled)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.state(id) == ModelState::Installed
    }

    fn set(&self, id: &str, s: ModelState) {
        self.inner.tx.send_modify(|m| {
            Arc::make_mut(m).insert(id.to_string(), s);
        });
    }

    /// Re-read the disk for every model that is not currently downloading.
    pub fn refresh(&self) {
        for spec in &self.inner.catalog {
            if self.state(&spec.id).is_active() {
                continue;
            }
            self.set(&spec.id, self.scan(spec));
        }
    }

    fn scan(&self, spec: &ModelSpec) -> ModelState {
        let dir = self.model_dir(&spec.id);
        let (mut done, mut complete) = (0u64, true);
        for f in &spec.files {
            let len = |p: PathBuf| std::fs::metadata(p).map(|m| m.len()).ok();
            match len(dir.join(&f.name)) {
                Some(n) if n == f.size => done += n,
                _ => {
                    complete = false;
                    done += len(dir.join(format!("{}.part", f.name))).unwrap_or(0).min(f.size);
                }
            }
        }
        if complete {
            ModelState::Installed
        } else if done > 0 {
            ModelState::Paused { done, total: spec.total_size() }
        } else {
            ModelState::NotInstalled
        }
    }

    pub fn storage(&self) -> Storage {
        let used = self
            .inner
            .catalog
            .iter()
            .map(|s| match self.state(&s.id) {
                ModelState::Installed => s.total_size(),
                ModelState::Paused { done, .. }
                | ModelState::Downloading { done, .. }
                | ModelState::Verifying { done, .. }
                | ModelState::Failed { done, .. } => done,
                ModelState::NotInstalled => 0,
            })
            .sum();
        Storage { dir: self.dir(), used_bytes: used }
    }

    /// Start (or resume) a download. Returns immediately; observe progress with [`subscribe`](Self::subscribe).
    pub fn start_download(&self, id: &str) -> Result<(), ManagerError> {
        let spec = self.spec(id).ok_or_else(|| ManagerError::UnknownModel(id.into()))?.clone();
        let mut jobs = self.inner.jobs.lock().expect("manager mutex poisoned");
        if jobs.contains_key(id) || self.is_installed(id) {
            return Ok(());
        }
        if self.inner.moving.load(Ordering::SeqCst) {
            return Err(ManagerError::FolderMoving);
        }
        let job = Job { cancel: CancellationToken::new(), paused: Arc::new(AtomicBool::new(false)) };
        let (cancel, paused) = (job.cancel.clone(), job.paused.clone());
        jobs.insert(id.to_string(), job);
        drop(jobs);

        let this = self.clone();
        let fut = async move {
            let _guard = JobGuard { inner: this.inner.clone(), spec: spec.clone() };
            let total = spec.total_size();
            let started = Instant::now();
            trace_log!(model = %spec.id, files = spec.files.len(), total_bytes = total, "model download started");
            let result = this.run_download(&spec, &cancel).await;
            let done_now = || match this.scan(&spec) {
                ModelState::Paused { done, .. } => done,
                ModelState::Installed => total,
                _ => 0,
            };
            let elapsed_ms = started.elapsed().as_millis() as u64;
            match result {
                Ok(()) => {
                    trace_log!(model = %spec.id, elapsed_ms, "model download finished");
                    this.set(&spec.id, ModelState::Installed)
                }
                Err(DlError::Cancelled) if paused.load(Ordering::SeqCst) => {
                    trace_log!(model = %spec.id, elapsed_ms, done_bytes = done_now(), "model download paused");
                    this.set(&spec.id, ModelState::Paused { done: done_now(), total })
                }
                Err(DlError::Cancelled) => {
                    trace_log!(model = %spec.id, elapsed_ms, "model download cancelled");
                    this.remove_partials(&spec);
                    this.set(&spec.id, ModelState::NotInstalled);
                }
                Err(e) => {
                    tracing::warn!("download of {} failed: {e}", spec.id);
                    trace_log!(model = %spec.id, elapsed_ms, done_bytes = done_now(), error = %e, "model download failed");
                    this.set(&spec.id, ModelState::Failed { message: e.to_string(), done: done_now(), total });
                }
            }
        };
        match &self.inner.rt {
            Some(rt) => rt.spawn(fut),
            None => tokio::spawn(fut),
        };
        Ok(())
    }

    async fn run_download(&self, spec: &ModelSpec, cancel: &CancellationToken) -> Result<(), DlError> {
        let dir = self.model_dir(&spec.id);
        tokio::fs::create_dir_all(&dir).await?;
        let lock = std::fs::File::options().create(true).append(true).open(dir.join(".download.lock"))?;
        loop {
            if cancel.is_cancelled() {
                return Err(DlError::Cancelled);
            }
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => tokio::select! {
                    _ = cancel.cancelled() => return Err(DlError::Cancelled),
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
                },
                Err(error) => return Err(DlError::Io(error.to_string())),
            }
        }
        if self.scan(spec) == ModelState::Installed {
            return Ok(());
        }
        let total = spec.total_size();
        let already = match self.scan(spec) {
            ModelState::Paused { done, .. } => done,
            _ => 0,
        };
        let need = total.saturating_sub(already);
        if free_space(&dir) < need + FREE_MARGIN {
            return Err(DlError::InsufficientSpace);
        }

        let manifest = self.resolve_manifest(cancel).await?;
        let allow_loopback = self.inner.manifest_url.as_deref().is_some_and(manifest::is_loopback_http);

        let mut completed: u64 = 0;
        for f in &spec.files {
            let dest = dir.join(&f.name);
            if tokio::fs::metadata(&dest).await.map(|m| m.len() == f.size).unwrap_or(false) {
                completed += f.size;
                continue;
            }
            let part = dir.join(format!("{}.part", f.name));
            let id = spec.id.clone();
            let this = self.clone();
            let base = completed;
            let f_size = f.size;
            let mut milestone = milestone_reached(base, total);
            let mut report = move |phase: download::Phase| match phase {
                download::Phase::Fetching(p) => {
                    let done = base + p.done;
                    if let Some(percent) = milestone(done) {
                        trace_log!(model = %id, percent, done_bytes = done, total_bytes = total, "model download progress");
                    }
                    this.set(&id, ModelState::Downloading { done, total, bytes_per_sec: p.bytes_per_sec });
                }
                download::Phase::Verifying => {
                    trace_log!(model = %id, "model download verifying");
                    this.set(&id, ModelState::Verifying { done: base + f_size, total });
                }
            };
            self.set(&spec.id, ModelState::Downloading { done: completed, total, bytes_per_sec: 0.0 });
            let candidates = match &manifest {
                Some(manifest) => manifest.candidate_urls(&spec.id, &f.name, &f.url, allow_loopback),
                None => vec![f.url.clone()],
            };
            self.fetch_from_candidates(spec, f, &candidates, &part, &dest, cancel, &mut report).await?;
            completed += f.size;
        }
        Ok(())
    }

    /// Fetches the static URL manifest only when a download starts; network or parse failures fall
    /// back silently to the compiled-in catalog URLs.
    async fn resolve_manifest(&self, cancel: &CancellationToken) -> Result<Option<Manifest>, DlError> {
        let Some(url) = self.inner.manifest_url.as_deref() else {
            return Ok(None);
        };
        let mut cache = self.inner.manifest_cache.lock().await;
        if let Some((fetched_at, manifest)) = cache.as_ref()
            && fetched_at.elapsed() < MANIFEST_TTL
        {
            return Ok(Some(manifest.clone()));
        }
        match manifest::fetch(&self.inner.client, url, cancel).await {
            Ok(manifest) => {
                trace_log!(url, models = manifest.models.len(), "model URL manifest loaded");
                *cache = Some((Instant::now(), manifest.clone()));
                Ok(Some(manifest))
            }
            Err(ManifestError::Cancelled) => Err(DlError::Cancelled),
            Err(error) => {
                tracing::warn!("could not refresh model download URLs from {url}: {error}");
                trace_log!(url, %error, "model URL manifest unavailable, using bundled URLs");
                Ok(None)
            }
        }
    }

    async fn invalidate_manifest_cache(&self) {
        *self.inner.manifest_cache.lock().await = None;
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_from_candidates(
        &self,
        spec: &ModelSpec,
        file: &crate::catalog::FileSpec,
        candidates: &[String],
        part: &Path,
        dest: &Path,
        cancel: &CancellationToken,
        report: &mut (dyn FnMut(download::Phase) + Send),
    ) -> Result<(), DlError> {
        let last_idx = candidates.len().saturating_sub(1);
        let mut last_err = None;
        for (idx, url) in candidates.iter().enumerate() {
            let target = download::Target { url, part, dest, size: file.size, sha256: file.sha256.as_deref() };
            match download::fetch(&self.inner.client, &target, cancel, report).await {
                Ok(()) => return Ok(()),
                Err(err @ (DlError::Cancelled | DlError::InsufficientSpace | DlError::Io(_))) => return Err(err),
                Err(err) => {
                    if idx < last_idx {
                        tracing::warn!(
                            "download of {}/{} failed from {url} ({err}), trying next URL",
                            spec.id,
                            file.name
                        );
                        trace_log!(
                            model = %spec.id,
                            file = %file.name,
                            attempt = idx + 1,
                            candidates = candidates.len(),
                            error = %err,
                            "model file URL failed, trying next URL"
                        );
                        // Drop mismatched partial bytes before trying a different mirror; single-URL
                        // downloads keep `.part` so retries can resume.
                        if matches!(err, DlError::SizeMismatch { .. }) {
                            let _ = tokio::fs::remove_file(part).await;
                        }
                    }
                    last_err = Some(err);
                }
            }
        }
        self.invalidate_manifest_cache().await;
        Err(last_err.expect("candidate list always includes the bundled URL"))
    }

    fn remove_partials(&self, spec: &ModelSpec) {
        let dir = self.model_dir(&spec.id);
        for f in &spec.files {
            let _ = std::fs::remove_file(dir.join(format!("{}.part", f.name)));
        }
    }

    /// Keeps the partial file so the download can resume.
    pub fn pause(&self, id: &str) {
        if let Some(j) = self.inner.jobs.lock().expect("manager mutex poisoned").get(id) {
            j.paused.store(true, Ordering::SeqCst);
            j.cancel.cancel();
        }
    }

    pub fn cancel(&self, id: &str) {
        let jobs = self.inner.jobs.lock().expect("manager mutex poisoned");
        if let Some(j) = jobs.get(id) {
            // Clear any pause request first: with `paused` still set the task's exit path
            // would keep the partial file, turning this cancel into a hidden pause.
            j.paused.store(false, Ordering::SeqCst);
            j.cancel.cancel();
        } else if !self.inner.moving.load(Ordering::SeqCst)
            && let Some(spec) = self.spec(id)
        {
            self.remove_partials(spec);
            self.set(id, ModelState::NotInstalled);
        }
    }

    pub fn delete(&self, id: &str) -> Result<(), ManagerError> {
        {
            let jobs = self.inner.jobs.lock().expect("manager mutex poisoned");
            if jobs.contains_key(id) {
                return Err(ManagerError::DownloadInProgress);
            }
            if self.inner.moving.load(Ordering::SeqCst) {
                return Err(ManagerError::FolderMoving);
            }
        }
        let dir = self.model_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        self.set(id, ModelState::NotInstalled);
        Ok(())
    }

    /// Move every installed (and partial) model to `new_dir` and make it the models folder. All or nothing: a move
    /// that fails part way leaves every model in the old folder, which stays the models folder, because a model left
    /// behind in a folder the settings do not name would read as not installed.
    pub fn change_dir(&self, new_dir: &Path) -> Result<(), ManagerError> {
        let _moving = self.begin_move()?;
        let old = self.dir();
        if old == new_dir {
            return Ok(());
        }
        let moving: Vec<(PathBuf, PathBuf)> = self
            .inner
            .catalog
            .iter()
            .map(|spec| (old.join(&spec.id), new_dir.join(&spec.id)))
            .filter(|(from, _)| from.exists())
            .collect();
        if moving.iter().any(|(from, _)| new_dir.starts_with(from)) {
            return Err(ManagerError::TargetInsideModel);
        }
        // Every target is then created by this move, so undoing it removes nothing that was there before.
        if let Some((_, taken)) = moving.iter().find(|(_, to)| to.symlink_metadata().is_ok()) {
            return Err(ManagerError::TargetExists(taken.clone()));
        }
        std::fs::create_dir_all(new_dir)?;
        if same_volume(&old, new_dir) {
            rename_all(&moving)?;
        } else {
            copy_all(&moving, new_dir)?;
        }
        let saved = self.inner.config.update(|c| c.models_dir = Some(new_dir.to_path_buf()));
        if let Err(e) = saved {
            undo_move(&moving);
            return Err(ManagerError::Config(format!("{e:#}")));
        }
        for (from, _) in &moving {
            if from.exists()
                && let Err(e) = std::fs::remove_dir_all(from)
            {
                tracing::warn!("models moved, but the old copy at {} stays: {e}", from.display());
            }
        }
        self.refresh();
        Ok(())
    }

    /// A move can take minutes and runs off the UI thread, so downloads, deletes and a second move are refused
    /// until it ends.
    fn begin_move(&self) -> Result<MoveGuard<'_>, ManagerError> {
        let jobs = self.inner.jobs.lock().expect("manager mutex poisoned");
        if !jobs.is_empty() {
            return Err(ManagerError::DownloadsRunning);
        }
        if self.inner.moving.swap(true, Ordering::SeqCst) {
            return Err(ManagerError::FolderMoving);
        }
        Ok(MoveGuard(&self.inner.moving))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_else_changes_the_models_folder_while_it_is_being_moved() {
        let dir = tempfile::tempdir().unwrap();
        let config = Arc::new(ConfigStore::open(dir.path().join("config.toml")).unwrap());
        config.update(|c| c.models_dir = Some(dir.path().join("models"))).unwrap();
        let manager = ModelManager::new(config);
        let id = manager.catalog()[0].id.clone();
        let elsewhere = dir.path().join("elsewhere");

        let moving = manager.begin_move().unwrap();
        assert!(matches!(manager.start_download(&id), Err(ManagerError::FolderMoving)));
        assert!(matches!(manager.delete(&id), Err(ManagerError::FolderMoving)));
        assert!(matches!(manager.change_dir(&elsewhere), Err(ManagerError::FolderMoving)));
        drop(moving);

        manager.change_dir(&elsewhere).unwrap();
        assert!(manager.begin_move().is_ok(), "a finished move releases the folder");
    }

    #[test]
    fn a_move_that_fails_part_way_puts_every_model_back() {
        let dir = tempfile::tempdir().unwrap();
        let (old, new) = (dir.path().join("old"), dir.path().join("new"));
        for id in ["a", "b"] {
            std::fs::create_dir_all(old.join(id)).unwrap();
            std::fs::write(old.join(id).join("m.bin"), id).unwrap();
        }
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("b"), "a file where the model would go").unwrap();
        let moving = [(old.join("a"), new.join("a")), (old.join("b"), new.join("b"))];
        assert!(rename_all(&moving).is_err());
        assert!(old.join("a").join("m.bin").exists() && !new.join("a").exists(), "the renamed model came back");

        assert!(copy_all(&moving, &new).is_err());
        assert!(old.join("a").join("m.bin").exists() && !new.join("a").exists(), "the copy was removed");
        assert!(old.join("b").join("m.bin").exists());
        assert_eq!(std::fs::read_to_string(new.join("b")).unwrap(), "a file where the model would go");
        assert_eq!(std::fs::read_dir(&new).unwrap().count(), 1, "no staging folder is left behind");
    }

    #[test]
    fn a_target_that_already_exists_is_refused_and_left_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let config = Arc::new(ConfigStore::open(dir.path().join("config.toml")).unwrap());
        let models = dir.path().join("models");
        config.update(|c| c.models_dir = Some(models.clone())).unwrap();
        let manager = ModelManager::new(config.clone());
        let id = manager.catalog()[0].id.clone();
        std::fs::create_dir_all(models.join(&id)).unwrap();
        std::fs::write(models.join(&id).join("m.bin"), "model").unwrap();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(elsewhere.join(&id)).unwrap();
        std::fs::write(elsewhere.join(&id).join("keep.txt"), "the user's file").unwrap();

        assert!(matches!(manager.change_dir(&elsewhere), Err(ManagerError::TargetExists(_))));
        assert_eq!(config.get().models_dir.as_deref(), Some(models.as_path()));
        assert_eq!(std::fs::read_to_string(elsewhere.join(&id).join("keep.txt")).unwrap(), "the user's file");
        assert!(models.join(&id).join("m.bin").exists());
    }

    #[test]
    fn a_folder_inside_a_model_is_refused_and_nothing_moves() {
        let dir = tempfile::tempdir().unwrap();
        let config = Arc::new(ConfigStore::open(dir.path().join("config.toml")).unwrap());
        let models = dir.path().join("models");
        config.update(|c| c.models_dir = Some(models.clone())).unwrap();
        let manager = ModelManager::new(config.clone());
        let id = manager.catalog()[0].id.clone();
        std::fs::create_dir_all(models.join(&id)).unwrap();

        let inside = models.join(&id).join("Dito Models");
        assert!(matches!(manager.change_dir(&inside), Err(ManagerError::TargetInsideModel)));
        assert_eq!(config.get().models_dir.as_deref(), Some(models.as_path()));
        assert!(!inside.exists());
    }

    #[test]
    fn eta_rounds_up_and_needs_a_measurable_speed() {
        let downloading = |bytes_per_sec| ModelState::Downloading { done: 100, total: 1_100, bytes_per_sec };
        assert_eq!(downloading(300.0).eta_secs(), Some(4));
        assert_eq!(downloading(0.5).eta_secs(), None);
        assert_eq!(ModelState::Installed.eta_secs(), None);
    }

    #[test]
    fn progress_is_reported_once_per_ten_percent_and_never_for_an_empty_total() {
        let mut reached = milestone_reached(0, 1_000);
        let seen: Vec<_> = [50, 99, 100, 105, 250, 255, 1_000].into_iter().map(&mut reached).collect();
        assert_eq!(seen, [None, None, Some(10), None, Some(20), None, Some(100)]);
        assert_eq!(milestone_reached(300, 1_000)(305), None, "a resumed download starts from where it was");
        assert_eq!(milestone_reached(0, 0)(10), None);
    }
}
