use kumo::{
    fetch::{Fetcher, MockFetcher},
    middleware::FetchRequest,
};

fn req(url: &str) -> FetchRequest {
    FetchRequest::new(url, 0)
}

#[tokio::test]
async fn returns_registered_response() {
    let mock = MockFetcher::new().with_response("https://example.com", 200, "<h1>Hello</h1>");
    let res = mock.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), Some("<h1>Hello</h1>"));
    assert_eq!(res.url(), "https://example.com");
}

#[tokio::test]
async fn returns_404_for_unregistered_url() {
    let mock = MockFetcher::new();
    let res = mock
        .fetch(&req("https://example.com/unknown"))
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn default_response_used_for_unregistered_url() {
    let mock = MockFetcher::new().with_default(200, "<p>default</p>");
    let res = mock.fetch(&req("https://anything.com")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), Some("<p>default</p>"));
}

#[tokio::test]
async fn specific_response_beats_default() {
    let mock = MockFetcher::new()
        .with_response("https://example.com/specific", 200, "specific")
        .with_default(200, "default");
    let res = mock
        .fetch(&req("https://example.com/specific"))
        .await
        .unwrap();
    assert_eq!(res.text(), Some("specific"));
}

#[tokio::test]
async fn with_html_file_loads_from_disk() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "<h1>From file</h1>").unwrap();
    let mock = MockFetcher::new().with_html_file("https://example.com", tmp.path());
    let res = mock.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res.text(), Some("<h1>From file</h1>"));
}
