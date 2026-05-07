use std::time::Duration;

use kumo::{
    fetch::{CachingFetcher, Fetcher, MockFetcher},
    middleware::Request,
};

fn req(url: &str) -> Request {
    Request::new(url, 0)
}

#[tokio::test]
async fn first_request_fetches_from_inner() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = MockFetcher::new().with_response("https://example.com", 200, "<h1>Hello</h1>");
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    let res = cf.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), Some("<h1>Hello</h1>"));
}

#[tokio::test]
async fn second_request_served_from_cache() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = MockFetcher::new().with_response("https://example.com", 200, "from network");
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    cf.fetch(&req("https://example.com")).await.unwrap();
    let res2 = cf.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res2.text(), Some("from network"));
}

#[tokio::test]
async fn cache_file_is_created_after_fetch() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = MockFetcher::new().with_response("https://example.com", 200, "body");
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    cf.fetch(&req("https://example.com")).await.unwrap();

    let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(files.len(), 1);
}

#[tokio::test]
async fn expired_entry_is_refetched() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = MockFetcher::new()
        .with_response("https://example.com", 200, "body")
        .with_default(200, "refetched");
    let cf = CachingFetcher::new(inner, tmp.path())
        .unwrap()
        .ttl(Duration::from_secs(0));

    cf.fetch(&req("https://example.com")).await.unwrap();
    let res = cf.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res.status(), 200);
}
