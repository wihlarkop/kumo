#![cfg(feature = "persistence")]

use kumo::{
    CrawlRequest,
    frontier::{FileFrontier, Frontier},
    request::FrontierRequest,
};
use reqwest::header::{HeaderName, HeaderValue};
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn new_frontier_is_empty() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    assert!(f.is_empty().await);
}

#[tokio::test]
async fn push_and_pop() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    assert!(f.push("https://example.com".into(), 0).await);
    let item = f.pop().await.unwrap();
    assert_eq!(item.0, "https://example.com");
    assert_eq!(item.1, 0);
    assert_eq!(item.2, 0);
}

#[tokio::test]
async fn deduplication_works() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    assert!(f.push("https://example.com".into(), 0).await);
    assert!(!f.push("https://example.com".into(), 0).await);
    assert_eq!(f.len().await, 1);
}

#[tokio::test]
async fn resumes_queue_from_disk() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        f.push("https://a.com".into(), 0).await;
        f.push("https://b.com".into(), 1).await;
        f.flush().await.unwrap();
    }
    let f2 = FileFrontier::open(dir.path()).unwrap();
    assert_eq!(f2.len().await, 2);
    let first = f2.pop().await.unwrap();
    assert_eq!(first.0, "https://a.com");
}

#[tokio::test]
async fn resumes_dedup_from_disk() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        f.push("https://a.com".into(), 0).await;
        f.flush().await.unwrap();
    }
    let f2 = FileFrontier::open(dir.path()).unwrap();
    f2.pop().await;
    assert!(!f2.push("https://a.com".into(), 0).await);
}

#[tokio::test]
async fn flush_replaces_state_files_atomically_without_temp_leftovers() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();

    f.push_request(CrawlRequest::get("https://example.com/a"), 0)
        .await;
    f.flush().await.unwrap();

    assert!(dir.path().join("queue.json").exists());
    assert!(dir.path().join("seen.json").exists());
    assert!(!dir.path().join("queue.json.tmp").exists());
    assert!(!dir.path().join("seen.json.tmp").exists());

    let f = FileFrontier::open(dir.path()).unwrap();
    assert_eq!(f.len().await, 1);
}

#[tokio::test]
async fn request_metadata_survives_flush_and_resume() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        f.push_request(
            CrawlRequest::post("https://example.com/api", br#"{"page":2}"#.to_vec())
                .header(
                    HeaderName::from_static("x-api-key"),
                    HeaderValue::from_static("secret"),
                )
                .priority(10)
                .meta("kind", "listing"),
            2,
        )
        .await;
        f.flush().await.unwrap();
    }

    let f = FileFrontier::open(dir.path()).unwrap();
    let queued = f.pop_request().await.unwrap();
    assert_eq!(queued.depth(), 2);
    assert_eq!(queued.request().url(), "https://example.com/api");
    assert_eq!(queued.request().method_ref(), reqwest::Method::POST);
    assert_eq!(queued.request().body_bytes(), Some(&br#"{"page":2}"#[..]));
    assert_eq!(
        queued.request().headers()["x-api-key"],
        HeaderValue::from_static("secret")
    );
    assert_eq!(queued.request().priority_value(), 10);
    assert_eq!(
        queued.request().meta_value("kind"),
        Some(&serde_json::json!("listing"))
    );
}

#[tokio::test]
async fn scheduled_retry_survives_flush_and_resume() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        f.push_request_force(
            FrontierRequest::new(CrawlRequest::get("https://example.com/retry"), 3, 2)
                .scheduled_after(Duration::from_secs(30)),
        )
        .await;
        f.flush().await.unwrap();
    }

    let f = FileFrontier::open(dir.path()).unwrap();
    let queued = f.pop_request().await.unwrap();
    assert_eq!(queued.depth(), 3);
    assert_eq!(queued.retry_count(), 2);
    assert_eq!(queued.request().url(), "https://example.com/retry");
    assert!(queued.scheduled_at().is_some());
}

#[tokio::test]
async fn dont_filter_allows_duplicate_url() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    assert!(
        f.push_request(CrawlRequest::get("https://example.com"), 0)
            .await
    );
    assert!(
        f.push_request(
            CrawlRequest::get("https://example.com").dont_filter(true),
            0,
        )
        .await
    );
    assert_eq!(f.len().await, 2);
}

#[tokio::test]
async fn flush_every_zero_disables_automatic_flush() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap().flush_every(0);

    assert!(f.push("https://example.com/a".into(), 0).await);
    assert!(!dir.path().join("queue.json").exists());
    assert!(!dir.path().join("seen.json").exists());

    f.flush().await.unwrap();
    assert!(dir.path().join("queue.json").exists());
    assert!(dir.path().join("seen.json").exists());
}

#[tokio::test]
async fn state_reports_loaded_queue_and_seen_counts() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        assert!(f.push("https://example.com/a".into(), 0).await);
        assert!(f.push("https://example.com/b".into(), 0).await);
        f.pop().await;
        f.flush().await.unwrap();
    }

    let f = FileFrontier::open(dir.path()).unwrap();
    let state = f.state().await;

    assert_eq!(state.queued, 1);
    assert_eq!(state.seen, 2);
    assert_eq!(state.dir, dir.path());
}
