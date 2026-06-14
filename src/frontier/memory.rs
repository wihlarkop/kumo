use super::Frontier;
use crate::request::{CrawlRequest, FrontierRequest};
use bloomfilter::Bloom;
use std::{cmp::Ordering, collections::BinaryHeap};
use tokio::sync::Mutex;

struct MemoryQueueEntry {
    queued: FrontierRequest,
}

impl PartialEq for MemoryQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.queued.request.priority_value() == other.queued.request.priority_value()
            && self.queued.sequence == other.queued.sequence
    }
}

impl Eq for MemoryQueueEntry {}

impl PartialOrd for MemoryQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MemoryQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.queued
            .request
            .priority_value()
            .cmp(&other.queued.request.priority_value())
            .then_with(|| other.queued.sequence.cmp(&self.queued.sequence))
    }
}

/// In-memory frontier: a priority queue + Bloom filter for O(1) URL deduplication.
pub struct MemoryFrontier {
    queue: Mutex<BinaryHeap<MemoryQueueEntry>>,
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
            queue: Mutex::new(BinaryHeap::new()),
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
        self.queue.lock().await.push(MemoryQueueEntry {
            queued: FrontierRequest::new(request, depth, 0),
        });
        true
    }

    async fn push_request_force(&self, queued: FrontierRequest) {
        self.queue.lock().await.push(MemoryQueueEntry { queued });
    }

    async fn pop_request(&self) -> Option<FrontierRequest> {
        self.queue.lock().await.pop().map(|entry| entry.queued)
    }

    async fn pop_request_batch(&self, limit: usize) -> Vec<FrontierRequest> {
        let mut queue = self.queue.lock().await;
        let count = limit.min(queue.len());
        let mut requests = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(entry) = queue.pop() {
                requests.push(entry.queued);
            }
        }
        requests
    }

    async fn restore_request_batch(&self, requests: Vec<FrontierRequest>) {
        self.queue.lock().await.extend(
            requests
                .into_iter()
                .map(|queued| MemoryQueueEntry { queued }),
        );
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
