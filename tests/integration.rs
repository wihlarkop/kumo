//! Integration tests for CrawlEngine using a mock HTTP server.
//!
//! These tests run a full crawl against a `mockito` server to avoid
//! any network dependency.

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::Fetcher,
    middleware::{DefaultHeaders, Request, StatusRetry},
    pipeline::RequireFields,
    retry::RetryPolicy,
    spider::{Output, Spider},
    store::{ItemStore, StdoutStore},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};
use tokio_stream::StreamExt;

/// A simple in-memory store that collects items for assertions.
#[derive(Clone, Default)]
struct VecStore {
    items: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl VecStore {
    fn collected(&self) -> Vec<serde_json::Value> {
        self.items.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ItemStore for VecStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        self.items.lock().unwrap().push(item.clone());
        Ok(())
    }
}

// ── Spiders ──────────────────────────────────────────────────────────────────

struct SinglePageSpider {
    start: String,
}

#[async_trait::async_trait]
impl Spider for SinglePageSpider {
    type Item = serde_json::Value;
    fn name(&self) -> &str {
        "single-page"
    }
    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let title = res
            .css("h1")
            .first()
            .map(|el| el.text())
            .unwrap_or_default();
        Ok(Output::new().item(serde_json::json!({ "title": title })))
    }
}

struct PaginatedSpider {
    page1: String,
}

#[async_trait::async_trait]
impl Spider for PaginatedSpider {
    type Item = serde_json::Value;
    fn name(&self) -> &str {
        "paginated"
    }
    fn start_urls(&self) -> Vec<String> {
        vec![self.page1.clone()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let item = serde_json::json!({ "url": res.url() });
        let next = res
            .css("a.next")
            .first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().item(item);
        if let Some(url) = next {
            output = output.follow(url);
        }
        Ok(output)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn engine_scrapes_single_page() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>Hello Kumo</h1></body></html>")
        .create_async()
        .await;

    let store = VecStore::default();
    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .store(store.clone())
        .run(SinglePageSpider {
            start: server.url(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 1);
    assert_eq!(stats.errors, 0);
    assert_eq!(store.collected()[0]["title"], "Hello Kumo");
}

#[tokio::test]
async fn engine_follows_pagination() {
    let mut server = mockito::Server::new_async().await;
    let base = server.url();

    let _m1 = server
        .mock("GET", "/page/1")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(format!(
            r#"<html><body><a class="next" href="{base}/page/2">Next</a></body></html>"#
        ))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", "/page/2")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><p>Last page</p></body></html>")
        .create_async()
        .await;

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .store(VecStore::default())
        .run(PaginatedSpider {
            page1: format!("{base}/page/1"),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 2);
    assert_eq!(stats.items_scraped, 2);
    assert_eq!(stats.errors, 0);
}

#[tokio::test]
async fn middleware_injects_custom_user_agent() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>ok</h1></body></html>")
        .match_header("user-agent", "test-bot/1.0")
        .create_async()
        .await;

    struct AgentSpider(String);
    #[async_trait::async_trait]
    impl Spider for AgentSpider {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "agent"
        }
        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }
        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new())
        }
    }

    let stats = CrawlEngine::builder()
        .respect_robots_txt(false)
        .middleware(DefaultHeaders::new().user_agent("test-bot/1.0"))
        .store(StdoutStore)
        .run(AgentSpider(server.url()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.errors, 0);
    _mock.assert_async().await;
}

#[tokio::test]
async fn pipeline_drops_items_missing_required_field() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><p>no title here</p></body></html>")
        .create_async()
        .await;

    struct NoTitleSpider(String);
    #[async_trait::async_trait]
    impl Spider for NoTitleSpider {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "no-title"
        }
        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }
        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new().item(serde_json::json!({ "body": "hello" })))
        }
    }

    let store = VecStore::default();
    let stats = CrawlEngine::builder()
        .respect_robots_txt(false)
        .pipeline(RequireFields::new(&["title"]))
        .store(store.clone())
        .run(NoTitleSpider(server.url()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(
        stats.items_scraped, 0,
        "pipeline should have dropped the item"
    );
    assert!(store.collected().is_empty());
}

#[tokio::test]
async fn status_retry_retries_on_429_and_succeeds() {
    let mut server = mockito::Server::new_async().await;
    // First request → 429; second → 200
    let _m1 = server
        .mock("GET", "/")
        .with_status(429)
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>ok</h1></body></html>")
        .create_async()
        .await;

    struct RetrySpider(String);
    #[async_trait::async_trait]
    impl Spider for RetrySpider {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "retry"
        }
        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }
        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new())
        }
    }

    let stats = CrawlEngine::builder()
        .respect_robots_txt(false)
        .retry(1, std::time::Duration::from_millis(1))
        .middleware(StatusRetry::new())
        .store(StdoutStore)
        .run(RetrySpider(server.url()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.errors, 0);
}

// ── RetryPolicy tests ─────────────────────────────────────────────────────────

/// Returns `fail_times` × 503 responses then 200 for every subsequent call.
struct SequentialFetcher {
    calls: Arc<AtomicU32>,
    fail_times: u32,
}

#[async_trait::async_trait]
impl Fetcher for SequentialFetcher {
    async fn fetch(&self, req: &Request) -> Result<Response, KumoError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let status = if n < self.fail_times { 503 } else { 200 };
        Ok(Response::from_parts(
            req.url(),
            status,
            "<html><body><h1>ok</h1></body></html>",
        ))
    }
}

#[tokio::test]
async fn retry_policy_exhausted_counts_as_single_error() {
    // 3 retries + 1 initial = 4 total calls, all return 503 → 1 error reported.
    let calls = Arc::new(AtomicU32::new(0));
    let fetcher = SequentialFetcher {
        calls: calls.clone(),
        fail_times: 100,
    };

    struct S;
    #[async_trait::async_trait]
    impl Spider for S {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "retry-exhaust"
        }
        fn start_urls(&self) -> Vec<String> {
            vec!["https://example.com/".into()]
        }
        async fn parse(&self, _r: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new())
        }
    }

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .middleware(StatusRetry::new())
        .retry_policy(
            RetryPolicy::new(3)
                .base_delay(std::time::Duration::from_millis(1))
                .max_delay(std::time::Duration::from_millis(4)),
        )
        .respect_robots_txt(false)
        .store(StdoutStore)
        .run(S)
        .await
        .unwrap();

    assert_eq!(stats.errors, 1, "all retries exhausted = one error");
    assert_eq!(stats.pages_crawled, 0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "initial + 3 retries = 4 fetches"
    );
}

#[tokio::test]
async fn retry_policy_succeeds_on_later_attempt() {
    // Fails twice then returns 200 — should succeed with 0 errors.
    let calls = Arc::new(AtomicU32::new(0));
    let fetcher = SequentialFetcher {
        calls: calls.clone(),
        fail_times: 2,
    };

    struct S;
    #[async_trait::async_trait]
    impl Spider for S {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "retry-success"
        }
        fn start_urls(&self) -> Vec<String> {
            vec!["https://example.com/".into()]
        }
        async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
            let title = res.css("h1").first().map(|e| e.text()).unwrap_or_default();
            Ok(Output::new().item(serde_json::json!({ "title": title })))
        }
    }

    let store = VecStore::default();
    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .middleware(StatusRetry::new())
        .retry_policy(
            RetryPolicy::new(3)
                .base_delay(std::time::Duration::from_millis(1))
                .max_delay(std::time::Duration::from_millis(4)),
        )
        .respect_robots_txt(false)
        .store(store.clone())
        .run(S)
        .await
        .unwrap();

    assert_eq!(stats.errors, 0, "succeeded after 2 failures");
    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 1);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "2 failures + 1 success = 3 fetches"
    );
    assert_eq!(store.collected()[0]["title"], "ok");
}

// ── MockFetcher tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn mock_fetcher_runs_spider_without_network() {
    use kumo::fetch::MockFetcher;

    struct TitleSpider;

    #[async_trait::async_trait]
    impl Spider for TitleSpider {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "title"
        }
        fn start_urls(&self) -> Vec<String> {
            vec!["https://example.com".into()]
        }
        async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
            let title = res.css("h1").first().map(|e| e.text()).unwrap_or_default();
            Ok(Output::new().item(serde_json::json!({"title": title})))
        }
    }

    let store = VecStore::default();
    let mock = MockFetcher::new().with_response("https://example.com", 200, "<h1>Test Title</h1>");

    let stats = CrawlEngine::builder()
        .store(store.clone())
        .fetcher(mock)
        .respect_robots_txt(false)
        .run(TitleSpider)
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 1);
    let items = store.collected();
    assert_eq!(items[0]["title"], "Test Title");
}

// ── ItemStream tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn stream_yields_items_as_scraped() {
    use kumo::fetch::MockFetcher;

    struct TitleStreamSpider;
    #[async_trait::async_trait]
    impl Spider for TitleStreamSpider {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "title-stream"
        }
        fn start_urls(&self) -> Vec<String> {
            vec!["https://example.com".into()]
        }
        async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
            let title = res.css("h1").first().map(|e| e.text()).unwrap_or_default();
            Ok(Output::new().item(serde_json::json!({ "title": title })))
        }
    }

    let mock =
        MockFetcher::new().with_response("https://example.com", 200, "<h1>Stream Works</h1>");

    let mut stream = CrawlEngine::builder()
        .fetcher(mock)
        .respect_robots_txt(false)
        .stream(TitleStreamSpider)
        .await
        .unwrap();

    let item = stream.next().await.expect("stream should yield one item");
    assert_eq!(item["title"], "Stream Works");
    assert!(
        stream.next().await.is_none(),
        "stream should end after all items"
    );
}

#[tokio::test]
async fn dropping_stream_does_not_panic() {
    use kumo::fetch::MockFetcher;

    struct MultiPageSpider;
    #[async_trait::async_trait]
    impl Spider for MultiPageSpider {
        type Item = serde_json::Value;
        fn name(&self) -> &str {
            "multi-stream"
        }
        fn start_urls(&self) -> Vec<String> {
            vec![
                "https://example.com/1".into(),
                "https://example.com/2".into(),
            ]
        }
        async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new().item(serde_json::json!({ "url": res.url() })))
        }
    }

    let mock = MockFetcher::new().with_default(200, "<p>page</p>");

    let stream = CrawlEngine::builder()
        .fetcher(mock)
        .respect_robots_txt(false)
        .stream(MultiPageSpider)
        .await
        .unwrap();

    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // reaching here without panic = pass
}

#[tokio::test]
async fn response_from_file_loads_html() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "<h1>From File</h1>").unwrap();
    let res = Response::from_file("https://example.com", tmp.path()).unwrap();
    assert_eq!(res.url(), "https://example.com");
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), Some("<h1>From File</h1>"));
}
