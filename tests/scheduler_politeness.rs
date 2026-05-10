use std::time::Duration;

use kumo::{
    frontier::MemoryFrontier,
    request::CrawlRequest,
    scheduler::{CrawlScheduler, PolitenessPolicy},
};

#[test]
fn politeness_policy_defaults_are_conservative_but_non_blocking() {
    let policy = PolitenessPolicy::default();

    assert_eq!(policy.default_per_domain_concurrency(), 8);
    assert_eq!(policy.default_per_domain_delay(), None);
    assert_eq!(policy.jitter_range(), None);
    assert!(policy.respects_robots_crawl_delay());
}

#[test]
fn politeness_policy_builder_sets_values() {
    let policy = PolitenessPolicy::new()
        .per_domain_concurrency(2)
        .per_domain_delay(Duration::from_millis(500))
        .jitter(Duration::from_millis(100))
        .respect_robots_crawl_delay(false);

    assert_eq!(policy.default_per_domain_concurrency(), 2);
    assert_eq!(
        policy.default_per_domain_delay(),
        Some(Duration::from_millis(500))
    );
    assert_eq!(policy.jitter_range(), Some(Duration::from_millis(100)));
    assert!(!policy.respects_robots_crawl_delay());
}

#[tokio::test]
async fn same_domain_request_waits_for_delay_after_completion() {
    let frontier = MemoryFrontier::default();
    let scheduler = CrawlScheduler::new(
        frontier,
        PolitenessPolicy::new().per_domain_delay(Duration::from_millis(50)),
    );

    scheduler
        .push_request(CrawlRequest::get("https://example.com/a"), 0)
        .await;
    scheduler
        .push_request(CrawlRequest::get("https://example.com/b"), 0)
        .await;

    let first = scheduler.next_ready().await.unwrap();
    assert_eq!(first.request().url(), "https://example.com/a");

    scheduler.finish(&first).await;

    let before = std::time::Instant::now();
    let second = scheduler.next_ready().await.unwrap();

    assert!(before.elapsed() >= Duration::from_millis(45));
    assert_eq!(second.request().url(), "https://example.com/b");
}
