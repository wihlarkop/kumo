use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::{Fetcher, MockFetcher},
    middleware::FetchRequest,
    spider::{Output, Spider},
};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use tokio_stream::StreamExt;

#[tokio::test]
async fn stream_yields_items_as_scraped() {
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
}

#[tokio::test]
async fn dropping_stream_stops_background_crawl() {
    struct CountingFetcher {
        calls: Arc<AtomicU32>,
    }

    #[async_trait::async_trait]
    impl Fetcher for CountingFetcher {
        async fn fetch(&self, req: &FetchRequest) -> Result<Response, KumoError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Response::from_parts(req.url(), 200, "<html></html>"))
        }
    }

    struct EndlessSpider;

    #[async_trait::async_trait]
    impl Spider for EndlessSpider {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "endless-stream"
        }

        fn start_urls(&self) -> Vec<String> {
            vec!["https://example.com/page/0".into()]
        }

        async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
            let current = res
                .url()
                .rsplit('/')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or_default();
            Ok(Output::new()
                .item(serde_json::json!({ "page": current }))
                .follow(format!("https://example.com/page/{}", current + 1)))
        }
    }

    let calls = Arc::new(AtomicU32::new(0));
    let fetcher = CountingFetcher {
        calls: calls.clone(),
    };

    let mut stream = CrawlEngine::builder()
        .concurrency(1)
        .stream_buffer(1)
        .fetcher(fetcher)
        .respect_robots_txt(false)
        .stream(EndlessSpider)
        .await
        .unwrap();

    let first = stream.next().await.expect("stream should yield one item");
    assert_eq!(first["page"], 0);

    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(
        calls.load(Ordering::SeqCst) <= 2,
        "background crawl kept fetching after stream drop: {} calls",
        calls.load(Ordering::SeqCst)
    );
}
