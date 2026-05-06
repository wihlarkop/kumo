mod support;

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::MockFetcher,
    spider::{Output, Spider},
};
use support::VecStore;

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
async fn mock_fetcher_runs_spider_without_network() {
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
            Ok(Output::new().item(serde_json::json!({ "title": title })))
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

#[tokio::test]
async fn response_from_file_loads_html() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "<h1>From File</h1>").unwrap();
    let res = Response::from_file("https://example.com", tmp.path()).unwrap();
    assert_eq!(res.url(), "https://example.com");
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), Some("<h1>From File</h1>"));
}
