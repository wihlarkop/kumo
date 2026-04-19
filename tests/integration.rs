//! Integration tests for CrawlEngine using a mock HTTP server.
//!
//! These tests run a full crawl against a `mockito` server to avoid
//! any network dependency.

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    middleware::{DefaultHeaders, StatusRetry},
    pipeline::RequireFields,
    spider::{Output, Spider},
    store::{ItemStore, StdoutStore},
};
use std::sync::{Arc, Mutex};

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
    fn name(&self) -> &str { "single-page" }
    fn start_urls(&self) -> Vec<String> { vec![self.start.clone()] }

    async fn parse(&self, res: &Response) -> Result<Output, KumoError> {
        let title = res.css("h1").first().map(|el| el.text()).unwrap_or_default();
        Ok(Output::new().item(serde_json::json!({ "title": title }))?)
    }
}

struct PaginatedSpider {
    page1: String,
}

#[async_trait::async_trait]
impl Spider for PaginatedSpider {
    fn name(&self) -> &str { "paginated" }
    fn start_urls(&self) -> Vec<String> { vec![self.page1.clone()] }

    async fn parse(&self, res: &Response) -> Result<Output, KumoError> {
        let item = serde_json::json!({ "url": res.url });
        let next = res
            .css("a.next")
            .first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().item(item)?;
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
        .run(SinglePageSpider { start: server.url() })
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
        .run(PaginatedSpider { page1: format!("{base}/page/1") })
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
        fn name(&self) -> &str { "agent" }
        fn start_urls(&self) -> Vec<String> { vec![self.0.clone()] }
        async fn parse(&self, _res: &Response) -> Result<Output, KumoError> {
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
        fn name(&self) -> &str { "no-title" }
        fn start_urls(&self) -> Vec<String> { vec![self.0.clone()] }
        async fn parse(&self, _res: &Response) -> Result<Output, KumoError> {
            // Emits item missing "title"
            Ok(Output::new().item(serde_json::json!({ "body": "hello" }))?)
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
    assert_eq!(stats.items_scraped, 0, "pipeline should have dropped the item");
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
        fn name(&self) -> &str { "retry" }
        fn start_urls(&self) -> Vec<String> { vec![self.0.clone()] }
        async fn parse(&self, _res: &Response) -> Result<Output, KumoError> {
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
