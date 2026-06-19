use std::collections::VecDeque;
use std::time::Duration;

use kumo::{
    CrawlRequest,
    frontier::{DeadLetterReason, Frontier, FrontierLeaseId, MemoryFrontier},
    request::FrontierRequest,
};
use tokio::sync::Mutex;

#[tokio::test]
async fn default_lease_request_wraps_popped_request() {
    let frontier = MemoryFrontier::new(1000);
    frontier
        .push_request(CrawlRequest::get("https://example.com").priority(10), 3)
        .await;

    let lease = frontier
        .lease_request(Duration::from_secs(30))
        .await
        .expect("request should be leased");

    assert!(lease.id().as_str().starts_with("ephemeral-"));
    assert_eq!(lease.request().request().url(), "https://example.com");
    assert_eq!(lease.request().depth(), 3);
    assert_eq!(lease.request().request().priority_value(), 10);
    assert_eq!(lease.delivery_count(), 1);
    assert!(lease.expires_at().is_some());
    assert!(frontier.is_empty().await);
}

#[tokio::test]
async fn default_lease_lifecycle_methods_are_noops() {
    let frontier = MemoryFrontier::new(1000);
    let lease_id = FrontierLeaseId::new("lease-1");

    frontier.ack_lease(&lease_id).await.unwrap();
    frontier.release_lease(&lease_id).await.unwrap();
    frontier
        .dead_letter(&lease_id, DeadLetterReason::RetryExhausted)
        .await
        .unwrap();
}

#[tokio::test]
async fn frontier_implementations_can_keep_old_required_methods_only() {
    let frontier = LegacyStyleFrontier::default();
    assert!(frontier.push("https://example.com".to_string(), 2).await);

    let lease = frontier
        .lease_request(Duration::from_secs(5))
        .await
        .expect("default lease implementation should use pop_request");

    assert_eq!(lease.request().request().url(), "https://example.com");
    assert_eq!(lease.request().depth(), 2);
    assert_eq!(lease.request().retry_count(), 0);
}

#[test]
fn dead_letter_reason_has_stable_labels() {
    assert_eq!(DeadLetterReason::RetryExhausted.as_str(), "retry_exhausted");
    assert_eq!(DeadLetterReason::Failed.as_str(), "failed");
    assert_eq!(DeadLetterReason::Interrupted.as_str(), "interrupted");
    assert_eq!(
        DeadLetterReason::Custom("blocked".to_string()).as_str(),
        "blocked"
    );
}

#[derive(Default)]
struct LegacyStyleFrontier {
    queue: Mutex<VecDeque<FrontierRequest>>,
}

#[async_trait::async_trait]
impl Frontier for LegacyStyleFrontier {
    async fn push(&self, url: String, depth: usize) -> bool {
        self.queue
            .lock()
            .await
            .push_back(FrontierRequest::new(CrawlRequest::get(url), depth, 0));
        true
    }

    async fn push_force(&self, url: String, depth: usize, retry_count: u32) {
        self.queue.lock().await.push_back(FrontierRequest::new(
            CrawlRequest::get(url),
            depth,
            retry_count,
        ));
    }

    async fn pop(&self) -> Option<(String, usize, u32)> {
        self.queue.lock().await.pop_front().map(|queued| {
            (
                queued.request().url().to_string(),
                queued.depth(),
                queued.retry_count(),
            )
        })
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
