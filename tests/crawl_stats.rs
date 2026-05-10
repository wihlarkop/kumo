use kumo::{
    CrawlRequest,
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::MockFetcher,
    spider::{Output, Spider},
    stats::{CrawlReport, CrawlStats},
    store::StdoutStore,
};

#[test]
fn crawl_report_exposes_scheduler_counters() {
    let mut stats = CrawlStats::default();
    stats.record_scheduled("example.com");
    stats.record_deduped("example.com");
    stats.record_retry("example.com");
    stats.record_robots_blocked("example.com");

    let report = CrawlReport::from(stats);

    assert_eq!(report.scheduled, 1);
    assert_eq!(report.deduped, 1);
    assert_eq!(report.retries, 1);
    assert_eq!(report.robots_blocked, 1);
    assert_eq!(report.domains["example.com"].scheduled, 1);
    assert_eq!(report.domains["example.com"].deduped, 1);
    assert_eq!(report.domains["example.com"].retries, 1);
    assert_eq!(report.domains["example.com"].robots_blocked, 1);
}

struct DuplicateSpider {
    start: String,
    target: String,
}

#[async_trait::async_trait]
impl Spider for DuplicateSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "duplicate-stats"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        if res.url() == self.start {
            return Ok(Output::new()
                .request(CrawlRequest::get(&self.target))
                .request(CrawlRequest::get(&self.target)));
        }
        Ok(Output::new())
    }
}

#[tokio::test]
async fn engine_stats_count_scheduled_completed_and_deduped_requests() {
    let start = "https://example.com/start";
    let target = "https://example.com/target";
    let mock = MockFetcher::new()
        .with_response(start, 200, "<a>start</a>")
        .with_response(target, 200, "<h1>target</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .fetcher(mock)
        .store(StdoutStore)
        .run(DuplicateSpider {
            start: start.to_string(),
            target: target.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 2);
    assert_eq!(stats.scheduled, 2);
    assert_eq!(stats.deduped, 1);
    assert_eq!(stats.domains["example.com"].scheduled, 2);
    assert_eq!(stats.domains["example.com"].deduped, 1);
    assert_eq!(stats.domains["example.com"].completed, 2);
}
