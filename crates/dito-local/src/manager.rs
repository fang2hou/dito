//! Local model lifecycle on disk: scan, download (pause / resume / cancel), delete, move.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use dito_core::config::{ConfigStore, default_data_dir};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::catalog::{ModelSpec, default_catalog};
use crate::download::{self, DlError};

#[derive(Debug, Clone, PartialEq)]
pub enum ModelState {
    NotInstalled,
    Downloading {
        done: u64,
        total: u64,
        bytes_per_sec: f64,
    },
    /// All bytes are on disk; the SHA-256 pass is running (seconds for multi-GB files).
    /// Cancellable: a cancel here keeps the completed `.part` file.
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
    /// Seconds left at the current speed.
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
    pub free_bytes: u64,
}

struct Job {
    cancel: CancellationToken,
    paused: Arc<AtomicBool>,
}

/// Removes the job entry when the download task finishes — including on panic, where the
/// old future-tail cleanup never ran and left a dead job that ignored pause/cancel forever.
struct JobGuard {
    inner: Arc<Inner>,
    spec: ModelSpec,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.inner.jobs.lock().unwrap().remove(&self.spec.id);
        // Panic-safety: if the task died mid-flight the state is still active. Downgrade it
        // to Paused (bytes are on disk) so the UI shows a resumable row instead of a dead one.
        let mgr = ModelManager { inner: self.inner.clone() };
        if mgr.state(&self.spec.id).is_active() {
            mgr.set(&self.spec.id, mgr.scan(&self.spec));
        }
    }
}

struct Inner {
    catalog: Vec<ModelSpec>,
    config: Arc<ConfigStore>,
    tx: watch::Sender<Snapshot>,
    jobs: Mutex<HashMap<String, Job>>,
    client: reqwest::Client,
    /// Captured when the manager is built inside a runtime (the app enters its runtime around
    /// construction). `start_download` is called from UI threads, where there is no ambient
    /// runtime, so it spawns on this handle instead of `tokio::spawn`.
    rt: Option<tokio::runtime::Handle>,
}

#[derive(Clone)]
pub struct ModelManager {
    inner: Arc<Inner>,
}

/// Free bytes on the volume containing `path` (or its nearest existing ancestor).
pub fn free_space(path: &Path) -> u64 {
    let mut p = path;
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent,
            None => return 0,
        }
    }
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else { return 0 };
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` is a properly sized, writable statvfs.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0 { st.f_bavail as u64 * st.f_frsize as u64 } else { 0 }
}

/// Keep this much free after a download completes.
const FREE_MARGIN: u64 = 200 * 1024 * 1024;

impl ModelManager {
    pub fn new(config: Arc<ConfigStore>) -> Self {
        Self::with_catalog(default_catalog(), config)
    }

    pub fn with_catalog(catalog: Vec<ModelSpec>, config: Arc<ConfigStore>) -> Self {
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
                client,
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
        let dir = self.dir();
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
        Storage { free_bytes: free_space(&dir), dir, used_bytes: used }
    }

    /// Start (or resume) a download. Returns immediately; observe progress with [`subscribe`](Self::subscribe).
    pub fn start_download(&self, id: &str) -> Result<()> {
        let spec = self.spec(id).with_context(|| format!("unknown model {id}"))?.clone();
        let mut jobs = self.inner.jobs.lock().unwrap();
        if jobs.contains_key(id) || self.is_installed(id) {
            return Ok(());
        }
        let job = Job { cancel: CancellationToken::new(), paused: Arc::new(AtomicBool::new(false)) };
        let (cancel, paused) = (job.cancel.clone(), job.paused.clone());
        jobs.insert(id.to_string(), job);
        drop(jobs);

        let this = self.clone();
        let fut = async move {
            let _guard = JobGuard { inner: this.inner.clone(), spec: spec.clone() };
            let result = this.run_download(&spec, &cancel).await;
            let total = spec.total_size();
            let done_now = || match this.scan(&spec) {
                ModelState::Paused { done, .. } => done,
                ModelState::Installed => total,
                _ => 0,
            };
            match result {
                Ok(()) => this.set(&spec.id, ModelState::Installed),
                Err(DlError::Cancelled) if paused.load(Ordering::SeqCst) => {
                    this.set(&spec.id, ModelState::Paused { done: done_now(), total })
                }
                Err(DlError::Cancelled) => {
                    this.remove_partials(&spec);
                    this.set(&spec.id, ModelState::NotInstalled);
                }
                Err(e) => {
                    tracing::warn!("download of {} failed: {e}", spec.id);
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
        let total = spec.total_size();
        let already = match self.scan(spec) {
            ModelState::Paused { done, .. } => done,
            _ => 0,
        };
        let need = total.saturating_sub(already);
        if free_space(&dir) < need + FREE_MARGIN {
            return Err(DlError::Io("not enough free disk space".into()));
        }

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
            let mut report = move |phase: download::Phase| match phase {
                download::Phase::Fetching(p) => {
                    this.set(
                        &id,
                        ModelState::Downloading { done: base + p.done, total, bytes_per_sec: p.bytes_per_sec },
                    );
                }
                download::Phase::Verifying => {
                    this.set(&id, ModelState::Verifying { done: base + f_size, total });
                }
            };
            self.set(&spec.id, ModelState::Downloading { done: completed, total, bytes_per_sec: 0.0 });
            let target =
                download::Target { url: &f.url, part: &part, dest: &dest, size: f.size, sha256: f.sha256.as_deref() };
            download::fetch(&self.inner.client, &target, cancel, &mut report).await?;
            completed += f.size;
        }
        Ok(())
    }

    fn remove_partials(&self, spec: &ModelSpec) {
        let dir = self.model_dir(&spec.id);
        for f in &spec.files {
            let _ = std::fs::remove_file(dir.join(format!("{}.part", f.name)));
        }
    }

    /// Stop a running download but keep the partial file so it can resume.
    pub fn pause(&self, id: &str) {
        if let Some(j) = self.inner.jobs.lock().unwrap().get(id) {
            j.paused.store(true, Ordering::SeqCst);
            j.cancel.cancel();
        }
    }

    /// Abort a download and discard what was fetched.
    pub fn cancel(&self, id: &str) {
        if let Some(j) = self.inner.jobs.lock().unwrap().get(id) {
            // Clear any pause request first: with `paused` still set the task's exit path
            // would keep the partial file, turning this cancel into a hidden pause.
            j.paused.store(false, Ordering::SeqCst);
            j.cancel.cancel();
        } else if let Some(spec) = self.spec(id) {
            self.remove_partials(spec);
            self.set(id, ModelState::NotInstalled);
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        if self.inner.jobs.lock().unwrap().contains_key(id) {
            bail!("download in progress");
        }
        let dir = self.model_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        self.set(id, ModelState::NotInstalled);
        Ok(())
    }

    /// Move every installed (and partial) model to `new_dir` and make it the models folder.
    pub fn change_dir(&self, new_dir: &Path) -> Result<()> {
        if !self.inner.jobs.lock().unwrap().is_empty() {
            bail!("downloads are running");
        }
        let old = self.dir();
        if old == new_dir {
            return Ok(());
        }
        std::fs::create_dir_all(new_dir)?;
        for spec in &self.inner.catalog {
            let from = old.join(&spec.id);
            if !from.exists() {
                continue;
            }
            let to = new_dir.join(&spec.id);
            if std::fs::rename(&from, &to).is_err() {
                copy_dir(&from, &to)?;
                std::fs::remove_dir_all(&from)?;
            }
        }
        let new_dir = new_dir.to_path_buf();
        self.inner.config.update(|c| c.models_dir = Some(new_dir))?;
        self.refresh();
        Ok(())
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}
