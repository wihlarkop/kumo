use std::time::{Duration, Instant};

use kumo::{
    frontier::MemoryFrontier,
    request::{CrawlRequest, FrontierRequest},
    scheduler::{CrawlScheduler, PolitenessPolicy},
};

#[tokio::test]
async fn retry_request_is_not_ready_before_retry_delay() {
    let frontier = MemoryFrontier::default();
    let scheduler = CrawlScheduler::new(frontier, PolitenessPolicy::default());

    scheduler
        .push_request_force(
            FrontierRequest::new(CrawlRequest::get("https://example.com/retry"), 0, 1)
                .scheduled_after(Duration::from_millis(50)),
        )
        .await;

    let before = Instant::now();
    let queued = scheduler.next_ready().await.unwrap();

    assert!(before.elapsed() >= Duration::from_millis(45));
    assert_eq!(queued.retry_count(), 1);
}
