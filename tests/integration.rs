//! Integration tests for CrawlEngine using a mock HTTP server.
//!
//! These tests run a full crawl against a `mockito` server to avoid
//! any network dependency.

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    spider::{Output, Spider},
    store::ItemStore,
};
use std::sync::{Arc, Mutex};

/// A simple in-memory store that collects items for assertions.
///
/// Uses `Arc<Mutex<...>>` internally so that cloning `VecStore` shares
/// the same underlying buffer — avoids the orphan rule that prevents
/// implementing foreign traits on `Arc<LocalType>`.
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

/// Spider that scrapes a single mock page.
struct SinglePageSpider {
    start: String,
}

#[async_trait::async_trait]
impl Spider for SinglePageSpider {
    fn name(&self) -> &str {
        "single-page"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        let title = res
            .css("h1")
            .first()
            .map(|el| el.text())
            .unwrap_or_default();
        Ok(Output::new().item(serde_json::json!({ "title": title })))
    }
}

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
    let store_clone = store.clone();

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .store(store_clone)
        .run(SinglePageSpider {
            start: server.url(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 1);
    assert_eq!(stats.errors, 0);

    let items = store.collected();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Hello Kumo");
}

/// Spider that follows one pagination link.
struct PaginatedSpider {
    page1: String,
    page2: String,
}

#[async_trait::async_trait]
impl Spider for PaginatedSpider {
    fn name(&self) -> &str {
        "paginated"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.page1.clone()]
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        let item = serde_json::json!({ "url": res.url });
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

#[tokio::test]
async fn engine_follows_pagination() {
    let mut server = mockito::Server::new_async().await;
    let base = server.url();

    let _mock1 = server
        .mock("GET", "/page/1")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(format!(
            r#"<html><body><a class="next" href="{}/page/2">Next</a></body></html>"#,
            base
        ))
        .create_async()
        .await;

    let _mock2 = server
        .mock("GET", "/page/2")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><p>Last page</p></body></html>")
        .create_async()
        .await;

    let store = VecStore::default();
    let store_clone = store.clone();

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .store(store_clone)
        .run(PaginatedSpider {
            page1: format!("{}/page/1", base),
            page2: format!("{}/page/2", base),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 2);
    assert_eq!(stats.items_scraped, 2);
    assert_eq!(stats.errors, 0);
}
