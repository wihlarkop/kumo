use kumo::{
    frontier::MemoryFrontier,
    request::CrawlRequest,
    scheduler::{CrawlScheduler, FingerprintPolicy, PolitenessPolicy},
};

#[test]
fn default_fingerprint_normalizes_scheme_host_and_fragment() {
    let policy = FingerprintPolicy::default();

    assert_eq!(
        policy
            .fingerprint("HTTPS://Example.COM/products/1#details")
            .unwrap(),
        "https://example.com/products/1"
    );
}

#[test]
fn fingerprint_policy_can_strip_tracking_params() {
    let policy = FingerprintPolicy::default().strip_tracking_params(true);

    assert_eq!(
        policy
            .fingerprint("https://example.com/a?utm_source=x&id=1&fbclid=y")
            .unwrap(),
        "https://example.com/a?id=1"
    );
}

#[tokio::test]
async fn scheduler_deduplicates_by_canonical_fingerprint() {
    let scheduler = CrawlScheduler::new(MemoryFrontier::default(), PolitenessPolicy::default());

    assert!(
        scheduler
            .push_request(CrawlRequest::get("https://example.com/a#one"), 0)
            .await
    );
    assert!(
        !scheduler
            .push_request(CrawlRequest::get("https://example.com/a#two"), 0)
            .await
    );

    let queued = scheduler.next_ready().await.unwrap();
    assert_eq!(queued.request().url(), "https://example.com/a#one");
    assert!(scheduler.next_ready().await.is_none());
}
