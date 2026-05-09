use kumo::{
    CrawlRequest,
    frontier::{Frontier, MemoryFrontier},
};

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
async fn higher_priority_pops_first() {
    let frontier = MemoryFrontier::new(1000);
    frontier
        .push_request(CrawlRequest::get("https://low.com").priority(-1), 0)
        .await;
    frontier
        .push_request(CrawlRequest::get("https://high.com").priority(10), 0)
        .await;
    frontier
        .push_request(CrawlRequest::get("https://mid.com").priority(2), 0)
        .await;

    assert_eq!(frontier.pop().await.unwrap().0, "https://high.com");
    assert_eq!(frontier.pop().await.unwrap().0, "https://mid.com");
    assert_eq!(frontier.pop().await.unwrap().0, "https://low.com");
}

#[tokio::test]
async fn equal_priority_preserves_fifo_order() {
    let frontier = MemoryFrontier::new(1000);
    frontier
        .push_request(CrawlRequest::get("https://a.com").priority(5), 0)
        .await;
    frontier
        .push_request(CrawlRequest::get("https://b.com").priority(5), 0)
        .await;
    frontier
        .push_request(CrawlRequest::get("https://c.com").priority(5), 0)
        .await;

    assert_eq!(frontier.pop().await.unwrap().0, "https://a.com");
    assert_eq!(frontier.pop().await.unwrap().0, "https://b.com");
    assert_eq!(frontier.pop().await.unwrap().0, "https://c.com");
}

#[tokio::test]
async fn dont_filter_allows_duplicate_url() {
    let frontier = MemoryFrontier::new(1000);
    assert!(
        frontier
            .push_request(CrawlRequest::get("https://example.com"), 0)
            .await
    );
    assert!(
        frontier
            .push_request(
                CrawlRequest::get("https://example.com").dont_filter(true),
                0,
            )
            .await
    );
    assert_eq!(frontier.len().await, 2);
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
    frontier
        .push_force("https://example.com".into(), 0, 1)
        .await;
    let _ = frontier.pop().await;
    let retried = frontier.pop().await.unwrap();
    assert_eq!(retried.0, "https://example.com");
    assert_eq!(retried.2, 1);
}
