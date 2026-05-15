use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use bloomfilter::Bloom;
use tokio::sync::Mutex;

use crate::{
    error::KumoError,
    request::{CrawlRequest, FrontierRequest, StoredFrontierRequest},
};

use super::Frontier;

const DEFAULT_FLUSH_EVERY: usize = 100;
const BLOOM_CAPACITY: usize = 1_000_000;

fn atomic_write(path: &Path, contents: &str) -> Result<(), KumoError> {
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json")
    ));

    std::fs::write(&tmp_path, contents)
        .map_err(|e| KumoError::store(format!("write {}", tmp_path.display()), e))?;

    match std::fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(_rename_error) if path.exists() => {
            std::fs::remove_file(path)
                .map_err(|e| KumoError::store(format!("replace {}", path.display()), e))?;
            std::fs::rename(&tmp_path, path)
                .map_err(|e| KumoError::store(format!("rename {}", path.display()), e))
        }
        Err(rename_error) => Err(KumoError::store(
            format!("rename {}", path.display()),
            rename_error,
        )),
    }
}

/// File-backed frontier that persists queue state to disk so a crawl can be
/// resumed after a crash or intentional stop.
///
/// State is stored in two JSON files inside `dir`:
/// - `queue.json` — pending URLs with depth and retry count
/// - `seen.json`  — all URLs ever enqueued (used to rebuild the Bloom filter on resume)
///
/// The state is flushed every `flush_every` pushes (default: 100). Remaining
/// unflushed state is also written when the engine calls `flush()` on the store
/// (end of crawl), though you should call `flush()` explicitly if you stop early.
///
/// # Example
/// ```rust,ignore
/// // Start a new crawl (or resume if state already exists):
/// CrawlEngine::builder()
///     .frontier(FileFrontier::open("./crawl-state")?)
/// ```
pub struct FileFrontier {
    queue: Mutex<VecDeque<FrontierRequest>>,
    seen_bloom: Mutex<Bloom<String>>,
    /// Exact list of seen URLs — persisted so the Bloom filter can be rebuilt on resume.
    seen_exact: Mutex<Vec<String>>,
    dir: PathBuf,
    flush_every: usize,
    push_count: AtomicUsize,
}

/// Snapshot of a [`FileFrontier`] in-memory state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFrontierState {
    /// Frontier directory containing `queue.json` and `seen.json`.
    pub dir: PathBuf,
    /// Number of pending requests currently in the queue.
    pub queued: usize,
    /// Number of exact seen request fingerprints loaded for deduplication.
    pub seen: usize,
    /// Number of pushes between automatic flushes. `0` means automatic flush is disabled.
    pub flush_every: usize,
}

impl std::fmt::Debug for FileFrontier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileFrontier")
            .field("dir", &self.dir)
            .field("flush_every", &self.flush_every)
            .finish()
    }
}

impl FileFrontier {
    /// Open a frontier backed by `dir`. If state files exist they are loaded
    /// automatically (resume); otherwise a fresh frontier is created.
    ///
    /// Uses exact URL storage for deduplication — unlike [`MemoryFrontier`](crate::frontier::MemoryFrontier),
    /// there are no false positives; every unique URL will be visited exactly once.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, KumoError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(|e| KumoError::store("create frontier dir", e))?;

        let queue_path = dir.join("queue.json");
        let seen_path = dir.join("seen.json");

        let mut bloom =
            Bloom::new_for_fp_rate(BLOOM_CAPACITY, 0.001).expect("valid bloom filter parameters");

        let seen_exact: Vec<String> = if seen_path.exists() {
            let data = std::fs::read_to_string(&seen_path)
                .map_err(|e| KumoError::store("read seen.json", e))?;
            let urls: Vec<String> =
                serde_json::from_str(&data).map_err(|e| KumoError::store("parse seen.json", e))?;
            for url in &urls {
                bloom.set(url);
            }
            urls
        } else {
            Vec::new()
        };

        let queue: VecDeque<FrontierRequest> = if queue_path.exists() {
            let data = std::fs::read_to_string(&queue_path)
                .map_err(|e| KumoError::store("read queue.json", e))?;
            let stored: VecDeque<StoredFrontierRequest> =
                serde_json::from_str(&data).map_err(|e| KumoError::store("parse queue.json", e))?;
            stored
                .into_iter()
                .map(FrontierRequest::try_from)
                .collect::<Result<_, _>>()
                .map_err(KumoError::store_msg)?
        } else {
            VecDeque::new()
        };

        Ok(Self {
            queue: Mutex::new(queue),
            seen_bloom: Mutex::new(bloom),
            seen_exact: Mutex::new(seen_exact),
            dir,
            flush_every: DEFAULT_FLUSH_EVERY,
            push_count: AtomicUsize::new(0),
        })
    }

    /// Override how often the state is flushed to disk (default: every 100 pushes).
    ///
    /// Set to `0` to disable automatic flushes and rely on explicit
    /// [`flush`](Self::flush) calls or the engine's final shutdown flush.
    pub fn flush_every(mut self, n: usize) -> Self {
        self.flush_every = n;
        self
    }

    /// Return a lightweight snapshot of the loaded frontier state.
    ///
    /// This is useful after opening a persisted frontier to confirm how much
    /// queued and deduplication state was recovered before resuming a crawl.
    pub async fn state(&self) -> FileFrontierState {
        let queued = self.queue.lock().await.len();
        let seen = self.seen_exact.lock().await.len();
        FileFrontierState {
            dir: self.dir.clone(),
            queued,
            seen,
            flush_every: self.flush_every,
        }
    }

    async fn flush_to_disk(&self) -> Result<(), KumoError> {
        let queue = self.queue.lock().await;
        let seen = self.seen_exact.lock().await;

        let stored_queue: VecDeque<StoredFrontierRequest> =
            queue.iter().map(StoredFrontierRequest::from).collect();
        let queue_json = serde_json::to_string(&stored_queue)
            .map_err(|e| KumoError::store("serialize queue", e))?;
        let seen_json =
            serde_json::to_string(&*seen).map_err(|e| KumoError::store("serialize seen", e))?;

        atomic_write(&self.dir.join("queue.json"), &queue_json)?;
        atomic_write(&self.dir.join("seen.json"), &seen_json)?;

        Ok(())
    }

    /// Flush current state to disk immediately. Call this before stopping the
    /// engine early if you want to resume the crawl later.
    pub async fn flush(&self) -> Result<(), KumoError> {
        self.flush_to_disk().await
    }
}

#[async_trait::async_trait]
impl Frontier for FileFrontier {
    async fn push(&self, url: String, depth: usize) -> bool {
        self.push_request(CrawlRequest::get(url), depth).await
    }

    async fn push_force(&self, url: String, depth: usize, retry_count: u32) {
        self.push_request_force(FrontierRequest::new(
            CrawlRequest::get(url),
            depth,
            retry_count,
        ))
        .await;
    }

    async fn pop(&self) -> Option<(String, usize, u32)> {
        self.pop_request().await.map(|queued| {
            (
                queued.request().url().to_string(),
                queued.depth(),
                queued.retry_count(),
            )
        })
    }

    async fn push_request(&self, request: CrawlRequest, depth: usize) -> bool {
        let url = request.dedup_key().to_string();
        if !request.dont_filter_enabled() {
            let maybe_seen = {
                let mut bloom = self.seen_bloom.lock().await;
                let maybe_seen = bloom.check(&url);
                if !maybe_seen {
                    bloom.set(&url);
                }
                maybe_seen
            };

            let mut seen = self.seen_exact.lock().await;
            if maybe_seen && seen.iter().any(|seen_url| seen_url == &url) {
                return false;
            }
            seen.push(url);
        }

        self.queue
            .lock()
            .await
            .push_back(FrontierRequest::new(request, depth, 0));

        let count = self.push_count.fetch_add(1, Ordering::Relaxed) + 1;
        if self.flush_every != 0 && count.is_multiple_of(self.flush_every) {
            self.flush_to_disk().await.ok();
        }
        true
    }

    async fn push_request_force(&self, queued: FrontierRequest) {
        self.queue.lock().await.push_back(queued);
        let count = self.push_count.fetch_add(1, Ordering::Relaxed) + 1;
        if self.flush_every != 0 && count.is_multiple_of(self.flush_every) {
            self.flush_to_disk().await.ok();
        }
    }

    async fn pop_request(&self) -> Option<FrontierRequest> {
        self.queue.lock().await.pop_front()
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }

    async fn flush(&self) -> Result<(), KumoError> {
        self.flush_to_disk().await
    }
}
