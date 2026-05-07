#![cfg(feature = "persistence")]

use kumo::frontier::{FileFrontier, Frontier};
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
