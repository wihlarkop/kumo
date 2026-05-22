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

#[test]
fn record_error_keeps_global_and_domain_failures_in_sync() {
    let mut stats = CrawlStats::default();
    stats.record_scheduled("example.com");
    stats.record_retry("example.com");
    stats.record_error("example.com");

    let report = CrawlReport::from(stats);

    assert_eq!(report.scheduled, 1);
    assert_eq!(report.retries, 1);
    assert_eq!(report.errors, 1);
    assert_eq!(report.domains["example.com"].scheduled, 1);
    assert_eq!(report.domains["example.com"].retries, 1);
    assert_eq!(report.domains["example.com"].failed, 1);
}

struct DuplicateSpider {
    start: String,
    target: String,
}

struct PanicSpider {
    start: String,
    name: &'static str,
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

#[async_trait::async_trait]
impl Spider for PanicSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        self.name
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
        panic!("intentional panic for stats coverage");
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

#[tokio::test]
async fn engine_stats_count_task_panic_as_domain_failure() {
    let start = "https://panic.example.com/start";
    let mock = MockFetcher::new().with_response(start, 200, "<h1>panic</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .fetcher(mock)
        .store(StdoutStore)
        .run(PanicSpider {
            start: start.to_string(),
            name: "panic-single",
        })
        .await
        .unwrap();

    assert_eq!(stats.errors, 1);
    assert_eq!(stats.pages_crawled, 0);
    assert_eq!(stats.domains["panic.example.com"].failed, 1);
}

#[tokio::test]
async fn run_all_stats_count_task_panic_for_the_right_spider() {
    let panic_url = "https://panic.example.com/start";
    let ok_url = "https://ok.example.com/start";
    let mock = MockFetcher::new()
        .with_response(panic_url, 200, "<h1>panic</h1>")
        .with_response(ok_url, 200, "<h1>ok</h1>");

    struct OkSpider(String);

    #[async_trait::async_trait]
    impl Spider for OkSpider {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "ok"
        }

        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }

        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new())
        }
    }

    let stats = CrawlEngine::builder()
        .concurrency(2)
        .respect_robots_txt(false)
        .fetcher(mock)
        .store(StdoutStore)
        .add_spider(PanicSpider {
            start: panic_url.to_string(),
            name: "panic-multi",
        })
        .add_spider(OkSpider(ok_url.to_string()))
        .run_all()
        .await
        .unwrap();

    assert_eq!(stats[0].errors, 1);
    assert_eq!(stats[0].pages_crawled, 0);
    assert_eq!(stats[0].domains["panic.example.com"].failed, 1);

    assert_eq!(stats[1].errors, 0);
    assert_eq!(stats[1].pages_crawled, 1);
    assert_eq!(stats[1].domains["ok.example.com"].completed, 1);
}
