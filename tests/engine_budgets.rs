use std::time::Duration;

use kumo::{
    CrawlRequest, StopReason,
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::MockFetcher,
    spider::{Output, Spider},
    store::StdoutStore,
};

struct ChainSpider {
    urls: Vec<String>,
}

#[async_trait::async_trait]
impl Spider for ChainSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "chain"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.urls[0].clone()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let mut output = Output::new().item(serde_json::json!({ "url": res.url() }));
        if let Some(position) = self.urls.iter().position(|url| url == res.url())
            && let Some(next) = self.urls.get(position + 1)
        {
            output = output.request(CrawlRequest::get(next));
        }
        Ok(output)
    }
}

struct ManyItemsSpider {
    start: String,
}

#[async_trait::async_trait]
impl Spider for ManyItemsSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "many-items"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new()
            .item(serde_json::json!({ "id": 1 }))
            .item(serde_json::json!({ "id": 2 }))
            .item(serde_json::json!({ "id": 3 })))
    }
}

struct ErrorSpider {
    start: String,
}

#[async_trait::async_trait]
impl Spider for ErrorSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "error"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
        Err(KumoError::parse_msg("intentional parse failure"))
    }
}

struct SinglePageSpider {
    start: String,
}

#[async_trait::async_trait]
impl Spider for SinglePageSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "single"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new().item(serde_json::json!({ "ok": true })))
    }
}

#[tokio::test]
async fn max_pages_stops_before_enqueueing_more_follows() {
    let urls = vec![
        "https://example.com/page/1".to_string(),
        "https://example.com/page/2".to_string(),
        "https://example.com/page/3".to_string(),
    ];
    let mock = MockFetcher::new()
        .with_response(&urls[0], 200, "<h1>one</h1>")
        .with_response(&urls[1], 200, "<h1>two</h1>")
        .with_response(&urls[2], 200, "<h1>three</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .max_pages(1)
        .fetcher(mock)
        .store(StdoutStore)
        .run(ChainSpider { urls })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 1);
    assert_eq!(stats.stop_reason, Some(StopReason::MaxPages));
}

#[tokio::test]
async fn max_items_stops_after_current_response_finishes() {
    let url = "https://example.com/items";
    let mock = MockFetcher::new().with_response(url, 200, "<h1>items</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .max_items(2)
        .fetcher(mock)
        .store(StdoutStore)
        .run(ManyItemsSpider {
            start: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 3);
    assert_eq!(stats.stop_reason, Some(StopReason::MaxItems));
}

#[tokio::test]
async fn max_duration_zero_stops_before_dispatching_requests() {
    let url = "https://example.com/slow";
    let mock = MockFetcher::new().with_response(url, 200, "<h1>slow</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .max_duration(Duration::ZERO)
        .fetcher(mock)
        .store(StdoutStore)
        .run(SinglePageSpider {
            start: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 0);
    assert_eq!(stats.items_scraped, 0);
    assert_eq!(stats.stop_reason, Some(StopReason::MaxDuration));
}

#[tokio::test]
async fn max_duration_wakes_single_spider_before_scheduler_delay() {
    let urls = vec![
        "https://example.com/page/1".to_string(),
        "https://example.com/page/2".to_string(),
    ];
    let mock = MockFetcher::new()
        .with_response(&urls[0], 200, "<h1>one</h1>")
        .with_response(&urls[1], 200, "<h1>two</h1>");

    let duration_budget = Duration::from_millis(500);
    let scheduler_delay = Duration::from_secs(2);
    let started = std::time::Instant::now();
    let stats = CrawlEngine::builder()
        .concurrency(1)
        .crawl_delay(scheduler_delay)
        .respect_robots_txt(false)
        .max_duration(duration_budget)
        .fetcher(mock)
        .store(StdoutStore)
        .run(ChainSpider { urls })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.stop_reason, Some(StopReason::MaxDuration));
    assert!(
        started.elapsed() < scheduler_delay,
        "duration budget waited for scheduler delay: {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn max_errors_stops_after_permanent_error() {
    let url = "https://example.com/error";
    let mock = MockFetcher::new().with_response(url, 200, "<h1>error</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .max_errors(1)
        .fetcher(mock)
        .store(StdoutStore)
        .run(ErrorSpider {
            start: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.errors, 1);
    assert_eq!(stats.pages_crawled, 0);
    assert_eq!(stats.stop_reason, Some(StopReason::MaxErrors));
}

#[tokio::test]
async fn run_all_applies_budgets_per_spider() {
    let one_page = "https://one.example.com/page/1".to_string();
    let chain_urls = vec![
        "https://chain.example.com/page/1".to_string(),
        "https://chain.example.com/page/2".to_string(),
        "https://chain.example.com/page/3".to_string(),
    ];
    let mock = MockFetcher::new()
        .with_response(&one_page, 200, "<h1>one</h1>")
        .with_response(&chain_urls[0], 200, "<h1>chain one</h1>")
        .with_response(&chain_urls[1], 200, "<h1>chain two</h1>")
        .with_response(&chain_urls[2], 200, "<h1>chain three</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(2)
        .respect_robots_txt(false)
        .max_pages(2)
        .fetcher(mock)
        .store(StdoutStore)
        .add_spider(SinglePageSpider { start: one_page })
        .add_spider(ChainSpider { urls: chain_urls })
        .run_all()
        .await
        .unwrap();

    assert_eq!(stats[0].pages_crawled, 1);
    assert_eq!(stats[0].stop_reason, Some(StopReason::FrontierExhausted));

    assert_eq!(stats[1].pages_crawled, 2);
    assert_eq!(stats[1].stop_reason, Some(StopReason::MaxPages));
}

#[tokio::test]
async fn max_duration_wakes_run_all_before_scheduler_delay() {
    let urls = vec![
        "https://example.com/page/1".to_string(),
        "https://example.com/page/2".to_string(),
    ];
    let mock = MockFetcher::new()
        .with_response(&urls[0], 200, "<h1>one</h1>")
        .with_response(&urls[1], 200, "<h1>two</h1>");

    let duration_budget = Duration::from_millis(500);
    let scheduler_delay = Duration::from_secs(2);
    let started = std::time::Instant::now();
    let stats = CrawlEngine::builder()
        .concurrency(1)
        .crawl_delay(scheduler_delay)
        .respect_robots_txt(false)
        .max_duration(duration_budget)
        .fetcher(mock)
        .store(StdoutStore)
        .add_spider(ChainSpider { urls })
        .run_all()
        .await
        .unwrap();

    assert_eq!(stats[0].pages_crawled, 1);
    assert_eq!(stats[0].stop_reason, Some(StopReason::MaxDuration));
    assert!(
        started.elapsed() < scheduler_delay,
        "duration budget waited for scheduler delay: {:?}",
        started.elapsed()
    );
}
