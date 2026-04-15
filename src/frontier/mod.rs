pub mod memory;

pub use memory::MemoryFrontier;

/// URL queue with deduplication. The frontier drives the crawl loop.
#[async_trait::async_trait]
pub trait Frontier: Send + Sync {
    /// Enqueue a URL if it has not been seen before.
    /// Returns `true` if added, `false` if it was a duplicate.
    async fn push(&self, url: String, depth: usize) -> bool;

    /// Dequeue the next URL. Returns `None` if the queue is currently empty.
    async fn pop(&self) -> Option<(String, usize)>;

    /// Number of URLs waiting in the queue.
    async fn len(&self) -> usize;
}
