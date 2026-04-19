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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontier::Frontier;

    #[tokio::test]
    async fn push_new_url_returns_true() {
        let frontier = MemoryFrontier::new(1000);
        assert!(frontier.push("https://example.com".into(), 0).await);
    }

    #[tokio::test]
    async fn push_duplicate_url_returns_false() {
        let frontier = MemoryFrontier::new(1000);
        frontier.push("https://example.com".into(), 0).await;
        assert!(!frontier.push("https://example.com".into(), 0).await);
    }

    #[tokio::test]
    async fn pop_empty_returns_none() {
        let frontier = MemoryFrontier::new(1000);
        assert!(frontier.pop().await.is_none());
    }

    #[tokio::test]
    async fn push_then_pop_returns_url_and_depth() {
        let frontier = MemoryFrontier::new(1000);
        frontier.push("https://example.com".into(), 3).await;
        let item = frontier.pop().await.unwrap();
        assert_eq!(item.0, "https://example.com");
        assert_eq!(item.1, 3);
        assert_eq!(item.2, 0);
    }

    #[tokio::test]
    async fn pop_is_fifo() {
        let frontier = MemoryFrontier::new(1000);
        frontier.push("https://a.com".into(), 0).await;
        frontier.push("https://b.com".into(), 0).await;
        frontier.push("https://c.com".into(), 0).await;
        assert_eq!(frontier.pop().await.unwrap().0, "https://a.com");
        assert_eq!(frontier.pop().await.unwrap().0, "https://b.com");
        assert_eq!(frontier.pop().await.unwrap().0, "https://c.com");
    }

    #[tokio::test]
    async fn len_reflects_queue_size() {
        let frontier = MemoryFrontier::new(1000);
        assert_eq!(frontier.len().await, 0);
        frontier.push("https://a.com".into(), 0).await;
        frontier.push("https://b.com".into(), 0).await;
        assert_eq!(frontier.len().await, 2);
        frontier.pop().await;
        assert_eq!(frontier.len().await, 1);
    }

    #[tokio::test]
    async fn is_empty_true_when_empty() {
        let frontier = MemoryFrontier::new(1000);
        assert!(frontier.is_empty().await);
        frontier.push("https://a.com".into(), 0).await;
        assert!(!frontier.is_empty().await);
    }

    #[tokio::test]
    async fn different_urls_are_not_deduplicated() {
        let frontier = MemoryFrontier::new(1000);
        assert!(frontier.push("https://a.com".into(), 0).await);
        assert!(frontier.push("https://b.com".into(), 0).await);
        assert_eq!(frontier.len().await, 2);
    }

    #[tokio::test]
    async fn push_force_bypasses_dedup_and_carries_retry_count() {
        let frontier = MemoryFrontier::new(1000);
        frontier.push("https://example.com".into(), 0).await;
        // Already seen, so push would return false — but push_force should work.
        frontier
            .push_force("https://example.com".into(), 0, 1)
            .await;
        // First pop is the original, second is the forced retry.
        let _ = frontier.pop().await;
        let retried = frontier.pop().await.unwrap();
        assert_eq!(retried.0, "https://example.com");
        assert_eq!(retried.2, 1);
    }
}
