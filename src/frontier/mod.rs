pub mod memory;

#[cfg(feature = "persistence")]
pub mod file;

pub use memory::MemoryFrontier;

#[cfg(feature = "persistence")]
pub use file::FileFrontier;

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

    /// Number of URLs waiting in the queue.
    async fn len(&self) -> usize;

    /// Returns `true` if the queue is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}
