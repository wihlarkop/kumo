use super::Frontier;
use bloomfilter::Bloom;
use std::collections::VecDeque;
use tokio::sync::Mutex;

/// In-memory frontier: a FIFO queue + Bloom filter for O(1) URL deduplication.
///
/// Configured for 1 million URLs at 0.1% false-positive rate.
/// Increase `num_items` in `Bloom::new_for_fp_rate` for larger crawls.
pub struct MemoryFrontier {
    queue: Mutex<VecDeque<(String, usize)>>,
    seen: Mutex<Bloom<String>>,
}

impl MemoryFrontier {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            seen: Mutex::new(
                Bloom::new_for_fp_rate(1_000_000, 0.001).expect("valid bloom filter parameters"),
            ),
        }
    }
}

impl Default for MemoryFrontier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Frontier for MemoryFrontier {
    async fn push(&self, url: String, depth: usize) -> bool {
        let mut seen = self.seen.lock().await;
        if seen.check(&url) {
            return false; // already seen — Bloom filter hit
        }
        seen.set(&url);
        drop(seen); // release lock before acquiring queue lock
        self.queue.lock().await.push_back((url, depth));
        true
    }

    async fn pop(&self) -> Option<(String, usize)> {
        self.queue.lock().await.pop_front()
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
