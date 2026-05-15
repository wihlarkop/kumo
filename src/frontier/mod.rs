pub mod memory;

#[cfg(feature = "persistence")]
pub mod file;

#[cfg(feature = "redis-frontier")]
pub mod redis_frontier;

pub use memory::MemoryFrontier;

#[cfg(feature = "persistence")]
pub use file::{FileFrontier, FileFrontierState};

#[cfg(feature = "redis-frontier")]
pub use redis_frontier::RedisFrontier;

use crate::error::KumoError;
use crate::request::{CrawlRequest, FrontierRequest};

/// URL queue with deduplication. The frontier drives the crawl loop.
#[async_trait::async_trait]
pub trait Frontier: Send + Sync {
    /// Enqueue a URL if it has not been seen before.
    /// Returns `true` if added, `false` if it was a duplicate.
    async fn push(&self, url: String, depth: usize) -> bool;

    /// Enqueue a URL unconditionally, bypassing the deduplication filter.
    /// Used by `ErrorPolicy::Retry` to re-queue a URL that previously failed.
    /// `retry_count` tracks how many times this URL has been retried.
    async fn push_force(&self, url: String, depth: usize, retry_count: u32);

    /// Dequeue the next URL. Returns `None` if the queue is currently empty.
    async fn pop(&self) -> Option<(String, usize, u32)>;

    /// Enqueue a crawl request if it has not been seen before.
    async fn push_request(&self, request: CrawlRequest, depth: usize) -> bool {
        self.push(request.url().to_string(), depth).await
    }

    /// Enqueue a crawl request unconditionally, bypassing deduplication.
    async fn push_request_force(&self, queued: FrontierRequest) {
        self.push_force(
            queued.request.url().to_string(),
            queued.depth,
            queued.retry_count,
        )
        .await;
    }

    /// Dequeue the next crawl request.
    async fn pop_request(&self) -> Option<FrontierRequest> {
        self.pop().await.map(|(url, depth, retry_count)| {
            FrontierRequest::new(CrawlRequest::get(url), depth, retry_count)
        })
    }

    /// Number of URLs waiting in the queue.
    async fn len(&self) -> usize;

    /// Returns `true` if the queue is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Flush any pending frontier state to durable storage.
    async fn flush(&self) -> Result<(), KumoError> {
        Ok(())
    }
}
