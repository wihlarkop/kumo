#![cfg(feature = "persistence")]

use kumo::{
    CrawlRequest,
    frontier::{DeadLetterReason, FileFrontier, Frontier},
    request::FrontierRequest,
    scheduler::{CrawlScheduler, FingerprintPolicy, PolitenessPolicy},
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
async fn stale_temp_files_do_not_affect_resume() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        assert!(f.push("https://example.com/a".into(), 0).await);
        f.flush().await.unwrap();
    }

    std::fs::write(dir.path().join("queue.json.tmp"), "not json").unwrap();
    std::fs::write(dir.path().join("seen.json.tmp"), "not json").unwrap();

    let f = FileFrontier::open(dir.path()).unwrap();
    let queued = f.pop().await.unwrap();
    assert_eq!(queued.0, "https://example.com/a");
    assert!(!f.push("https://example.com/a".into(), 0).await);
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
async fn dont_filter_survives_flush_and_resume() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        f.push_request(
            CrawlRequest::get("https://example.com/revisit").dont_filter(true),
            0,
        )
        .await;
        f.flush().await.unwrap();
    }

    let f = FileFrontier::open(dir.path()).unwrap();
    let queued = f.pop_request().await.unwrap();
    assert_eq!(queued.request().url(), "https://example.com/revisit");
    assert!(queued.request().dont_filter_enabled());
}

#[tokio::test]
async fn scheduler_dedup_fingerprint_survives_file_frontier_resume() {
    let dir = tempdir().unwrap();
    {
        let frontier = FileFrontier::open(dir.path()).unwrap();
        let scheduler = CrawlScheduler::new(frontier, PolitenessPolicy::default())
            .with_fingerprint_policy(FingerprintPolicy::default().strip_tracking_params(true));

        assert!(
            scheduler
                .push_request(
                    CrawlRequest::get("https://example.com/products?b=2&a=1&utm_source=test"),
                    0,
                )
                .await
        );
        scheduler.flush().await.unwrap();
    }

    let frontier = FileFrontier::open(dir.path()).unwrap();
    let scheduler = CrawlScheduler::new(frontier, PolitenessPolicy::default())
        .with_fingerprint_policy(FingerprintPolicy::default().strip_tracking_params(true));

    assert!(
        !scheduler
            .push_request(CrawlRequest::get("https://EXAMPLE.com/products?a=1&b=2"), 0,)
            .await
    );
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

#[tokio::test]
async fn lease_request_moves_request_out_of_queue_until_ack() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    f.push("https://example.com/leased".into(), 2).await;

    let lease = f.lease_request(Duration::from_secs(60)).await.unwrap();
    assert_eq!(
        lease.request().request().url(),
        "https://example.com/leased"
    );
    assert_eq!(lease.request().depth(), 2);

    let state = f.state().await;
    assert_eq!(state.queued, 0);
    let leases = std::fs::read_to_string(dir.path().join("leases.json")).unwrap();
    assert!(leases.contains("https://example.com/leased"));

    f.ack_lease(lease.id()).await.unwrap();
    let state = f.state().await;
    assert_eq!(state.queued, 0);
    let leases = std::fs::read_to_string(dir.path().join("leases.json")).unwrap();
    assert_eq!(leases, "[]");

    let reopened = FileFrontier::open(dir.path()).unwrap();
    assert!(reopened.is_empty().await);
}

#[tokio::test]
async fn release_lease_requeues_request() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    f.push_request(
        CrawlRequest::get("https://example.com/release").priority(5),
        4,
    )
    .await;

    let lease = f.lease_request(Duration::from_secs(60)).await.unwrap();
    f.release_lease(lease.id()).await.unwrap();

    let queued = f.pop_request().await.unwrap();
    assert_eq!(queued.request().url(), "https://example.com/release");
    assert_eq!(queued.request().priority_value(), 5);
    assert_eq!(queued.depth(), 4);
}

#[tokio::test]
async fn open_recovers_existing_leases_to_queue() {
    let dir = tempdir().unwrap();
    {
        let f = FileFrontier::open(dir.path()).unwrap();
        f.push("https://example.com/recover-lease".into(), 1).await;
        let lease = f.lease_request(Duration::from_secs(60)).await.unwrap();
        assert_eq!(
            lease.request().request().url(),
            "https://example.com/recover-lease"
        );
        f.flush().await.unwrap();
    }

    let reopened = FileFrontier::open(dir.path()).unwrap();
    let state = reopened.state().await;
    assert_eq!(state.queued, 1);

    let queued = reopened.pop().await.unwrap();
    assert_eq!(queued.0, "https://example.com/recover-lease");
}

#[tokio::test]
async fn expired_lease_is_reclaimed_to_queue() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    f.push("https://example.com/expired".into(), 0).await;

    let lease = f.lease_request(Duration::ZERO).await.unwrap();
    assert_eq!(
        lease.request().request().url(),
        "https://example.com/expired"
    );

    assert_eq!(f.len().await, 1);
    let queued = f.pop().await.unwrap();
    assert_eq!(queued.0, "https://example.com/expired");
}

#[tokio::test]
async fn dead_letter_moves_lease_to_dead_letter_file() {
    let dir = tempdir().unwrap();
    let f = FileFrontier::open(dir.path()).unwrap();
    f.push("https://example.com/dead".into(), 0).await;

    let lease = f.lease_request(Duration::from_secs(60)).await.unwrap();
    f.dead_letter(lease.id(), DeadLetterReason::Failed)
        .await
        .unwrap();

    let state = f.state().await;
    assert_eq!(state.queued, 0);

    let dead_letters = std::fs::read_to_string(dir.path().join("dead_letters.json")).unwrap();
    assert!(dead_letters.contains("https://example.com/dead"));
    assert!(dead_letters.contains("failed"));

    let reopened = FileFrontier::open(dir.path()).unwrap();
    assert!(reopened.is_empty().await);
}
