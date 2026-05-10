use super::Frontier;
use crate::request::{CrawlRequest, FrontierRequest};
use bloomfilter::Bloom;
use std::collections::VecDeque;
use tokio::sync::Mutex;

/// In-memory frontier: a priority queue + Bloom filter for O(1) URL deduplication.
pub struct MemoryFrontier {
    queue: Mutex<VecDeque<FrontierRequest>>,
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
                queued.request.url().to_string(),
                queued.depth,
                queued.retry_count,
            )
        })
    }

    async fn push_request(&self, request: CrawlRequest, depth: usize) -> bool {
        let mut seen = self.seen.lock().await;
        let seen_key = request.dedup_key().to_string();
        if !request.dont_filter_enabled() && seen.check(&seen_key) {
            return false;
        }
        if !request.dont_filter_enabled() {
            seen.set(&seen_key);
        }
        drop(seen);
        self.queue
            .lock()
            .await
            .push_back(FrontierRequest::new(request, depth, 0));
        true
    }

    async fn push_request_force(&self, queued: FrontierRequest) {
        self.queue.lock().await.push_back(queued);
    }

    async fn pop_request(&self) -> Option<FrontierRequest> {
        let mut queue = self.queue.lock().await;
        let best_idx = queue
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.request
                    .priority_value()
                    .cmp(&b.request.priority_value())
                    .then_with(|| b.sequence.cmp(&a.sequence))
            })
            .map(|(idx, _)| idx)?;
        queue.remove(best_idx)
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
