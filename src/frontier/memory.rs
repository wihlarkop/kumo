use super::Frontier;
use bloomfilter::Bloom;
use std::collections::VecDeque;
use tokio::sync::Mutex;

/// In-memory frontier: a FIFO queue + Bloom filter for O(1) URL deduplication.
pub struct MemoryFrontier {
    queue: Mutex<VecDeque<(String, usize, u32)>>,
    seen: Mutex<Bloom<String>>,
}

impl MemoryFrontier {
    /// Create a frontier sized for `expected_urls` unique URLs at 0.1% false-positive rate.
    ///
    /// **Note:** The Bloom filter used for deduplication can produce false positives.
    /// A small fraction (~0.1%) of unique URLs may be incorrectly treated as already-seen
    /// and silently skipped. For crawls that require 100% URL coverage, use
    /// [`FileFrontier`](crate::frontier::FileFrontier) (which stores exact URLs) or a
    /// custom `Frontier` implementation.
    pub fn new(expected_urls: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            seen: Mutex::new(
                Bloom::new_for_fp_rate(expected_urls, 0.001)
                    .expect("valid bloom filter parameters"),
            ),
        }
    }
}

impl Default for MemoryFrontier {
    fn default() -> Self {
        Self::new(1_000_000)
    }
}

#[async_trait::async_trait]
impl Frontier for MemoryFrontier {
    async fn push(&self, url: String, depth: usize) -> bool {
        let mut seen = self.seen.lock().await;
        if seen.check(&url) {
            return false;
        }
        seen.set(&url);
        drop(seen);
        self.queue.lock().await.push_back((url, depth, 0));
        true
    }

    async fn push_force(&self, url: String, depth: usize, retry_count: u32) {
        self.queue.lock().await.push_back((url, depth, retry_count));
    }

    async fn pop(&self) -> Option<(String, usize, u32)> {
        self.queue.lock().await.pop_front()
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
