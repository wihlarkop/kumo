use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use bloomfilter::Bloom;
use tokio::sync::Mutex;

use crate::error::KumoError;

use super::Frontier;

const DEFAULT_FLUSH_EVERY: usize = 100;
const BLOOM_CAPACITY: usize = 1_000_000;

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
    queue: Mutex<VecDeque<(String, usize, u32)>>,
    seen_bloom: Mutex<Bloom<String>>,
    /// Exact list of seen URLs — persisted so the Bloom filter can be rebuilt on resume.
    seen_exact: Mutex<Vec<String>>,
    dir: PathBuf,
    flush_every: usize,
    push_count: AtomicUsize,
}

impl FileFrontier {
    /// Open a frontier backed by `dir`. If state files exist they are loaded
    /// automatically (resume); otherwise a fresh frontier is created.
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

        let queue: VecDeque<(String, usize, u32)> = if queue_path.exists() {
            let data = std::fs::read_to_string(&queue_path)
                .map_err(|e| KumoError::store("read queue.json", e))?;
            serde_json::from_str(&data).map_err(|e| KumoError::store("parse queue.json", e))?
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
    pub fn flush_every(mut self, n: usize) -> Self {
        self.flush_every = n;
        self
    }

    async fn flush_to_disk(&self) -> Result<(), KumoError> {
        let queue = self.queue.lock().await;
        let seen = self.seen_exact.lock().await;

        let queue_json =
            serde_json::to_string(&*queue).map_err(|e| KumoError::store("serialize queue", e))?;
        let seen_json =
            serde_json::to_string(&*seen).map_err(|e| KumoError::store("serialize seen", e))?;

        std::fs::write(self.dir.join("queue.json"), queue_json)
            .map_err(|e| KumoError::store("write queue.json", e))?;
        std::fs::write(self.dir.join("seen.json"), seen_json)
            .map_err(|e| KumoError::store("write seen.json", e))?;

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
        let mut bloom = self.seen_bloom.lock().await;
        if bloom.check(&url) {
            return false;
        }
        bloom.set(&url);
        drop(bloom);

        self.seen_exact.lock().await.push(url.clone());
        self.queue.lock().await.push_back((url, depth, 0));

        let count = self.push_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count.is_multiple_of(self.flush_every) {
            self.flush_to_disk().await.ok();
        }
        true
    }

    async fn push_force(&self, url: String, depth: usize, retry_count: u32) {
        self.queue.lock().await.push_back((url, depth, retry_count));
        let count = self.push_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count.is_multiple_of(self.flush_every) {
            self.flush_to_disk().await.ok();
        }
    }

    async fn pop(&self) -> Option<(String, usize, u32)> {
        self.queue.lock().await.pop_front()
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn new_frontier_is_empty() {
        let dir = tempdir().unwrap();
        let f = FileFrontier::open(dir.path()).unwrap();
        assert!(f.is_empty().await);
    }

    #[tokio::test]
    async fn push_and_pop() {
        let dir = tempdir().unwrap();
        let f = FileFrontier::open(dir.path()).unwrap();
        assert!(f.push("https://example.com".into(), 0).await);
        let item = f.pop().await.unwrap();
        assert_eq!(item.0, "https://example.com");
        assert_eq!(item.1, 0);
        assert_eq!(item.2, 0);
    }

    #[tokio::test]
    async fn deduplication_works() {
        let dir = tempdir().unwrap();
        let f = FileFrontier::open(dir.path()).unwrap();
        assert!(f.push("https://example.com".into(), 0).await);
        assert!(!f.push("https://example.com".into(), 0).await);
        assert_eq!(f.len().await, 1);
    }

    #[tokio::test]
    async fn resumes_queue_from_disk() {
        let dir = tempdir().unwrap();
        {
            let f = FileFrontier::open(dir.path()).unwrap();
            f.push("https://a.com".into(), 0).await;
            f.push("https://b.com".into(), 1).await;
            f.flush().await.unwrap();
        }
        // Re-open — should resume with same queue.
        let f2 = FileFrontier::open(dir.path()).unwrap();
        assert_eq!(f2.len().await, 2);
        let first = f2.pop().await.unwrap();
        assert_eq!(first.0, "https://a.com");
    }

    #[tokio::test]
    async fn resumes_dedup_from_disk() {
        let dir = tempdir().unwrap();
        {
            let f = FileFrontier::open(dir.path()).unwrap();
            f.push("https://a.com".into(), 0).await;
            f.flush().await.unwrap();
        }
        // Re-open — already-seen URL should not be re-queued.
        let f2 = FileFrontier::open(dir.path()).unwrap();
        f2.pop().await; // drain
        assert!(!f2.push("https://a.com".into(), 0).await);
    }
}
