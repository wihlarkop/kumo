use kumo::{
    CrawlRequest,
    engine::CrawlEngine,
    error::{KumoError, KumoErrorKind},
    extract::Response,
    fetch::MockFetcher,
    middleware::{FetchRequest, Middleware},
    spider::{Output, Spider},
    stats::{CrawlReport, CrawlStats, CrawlTimingStats, StopReason},
    store::StdoutStore,
};
use std::time::Duration;

#[test]
fn crawl_report_exposes_scheduler_counters() {
    let mut stats = CrawlStats::default();
    stats.record_scheduled("example.com");
    stats.record_deduped("example.com");
    stats.record_retry("example.com");
    stats.record_retry_exhausted("example.com");
    stats.record_robots_blocked("example.com");

    let report = CrawlReport::from(stats);

    assert_eq!(report.scheduled, 1);
    assert_eq!(report.deduped, 1);
    assert_eq!(report.retries, 1);
    assert_eq!(report.retry_exhausted, 1);
    assert_eq!(report.robots_blocked, 1);
    assert_eq!(report.domains["example.com"].scheduled, 1);
    assert_eq!(report.domains["example.com"].deduped, 1);
    assert_eq!(report.domains["example.com"].retries, 1);
    assert_eq!(report.domains["example.com"].retry_exhausted, 1);
    assert_eq!(report.domains["example.com"].robots_blocked, 1);
}

#[test]
fn crawl_report_exposes_stop_reason() {
    let stats = CrawlStats {
        stop_reason: Some(StopReason::MaxPages),
        ..CrawlStats::default()
    };

    let report = CrawlReport::from(stats);

    assert_eq!(report.stop_reason, Some(StopReason::MaxPages));
}

#[test]
fn stop_reason_exports_stable_labels() {
    assert_eq!(StopReason::FrontierExhausted.as_str(), "frontier_exhausted");
    assert_eq!(StopReason::Interrupted.as_str(), "interrupted");
    assert_eq!(StopReason::MaxPages.as_str(), "max_pages");
    assert_eq!(StopReason::MaxItems.as_str(), "max_items");
    assert_eq!(StopReason::MaxDuration.as_str(), "max_duration");
    assert_eq!(StopReason::MaxErrors.as_str(), "max_errors");
}

#[test]
fn crawl_report_exports_stable_json() {
    let mut stats = CrawlStats {
        pages_crawled: 2,
        items_scraped: 3,
        errors: 1,
        duration: Duration::from_millis(1_500),
        bytes_downloaded: 42,
        timings: CrawlTimingStats {
            middleware_request: Duration::from_millis(11),
            fetch: Duration::from_millis(22),
            middleware_response: Duration::from_millis(33),
            parse: Duration::from_millis(44),
            pipeline: Duration::from_millis(55),
            store: Duration::from_millis(66),
        },
        stop_reason: Some(StopReason::MaxErrors),
        ..CrawlStats::default()
    };
    stats.record_scheduled("example.com");
    stats.record_deduped("example.com");
    stats.record_completed("example.com");
    stats.record_error_kind("example.com", KumoErrorKind::HttpStatus);
    stats.record_retry("example.com");
    stats.record_retry_exhausted("example.com");
    stats.record_robots_blocked("example.com");

    let report = CrawlReport::from(stats);
    let json = report.to_json_value();

    assert_eq!(json["pages_crawled"], 2);
    assert_eq!(json["items_scraped"], 3);
    assert_eq!(json["errors"], 2);
    assert_eq!(json["error_kinds"]["http_status"], 1);
    assert_eq!(json["duration_ms"], 1_500);
    assert_eq!(json["duration_secs"], 1.5);
    assert_eq!(json["timings"]["middleware_request_ms"], 11);
    assert_eq!(json["timings"]["middleware_request_secs"], 0.011);
    assert_eq!(json["timings"]["fetch_ms"], 22);
    assert_eq!(json["timings"]["fetch_secs"], 0.022);
    assert_eq!(json["timings"]["middleware_response_ms"], 33);
    assert_eq!(json["timings"]["middleware_response_secs"], 0.033);
    assert_eq!(json["timings"]["parse_ms"], 44);
    assert_eq!(json["timings"]["parse_secs"], 0.044);
    assert_eq!(json["timings"]["pipeline_ms"], 55);
    assert_eq!(json["timings"]["pipeline_secs"], 0.055);
    assert_eq!(json["timings"]["store_ms"], 66);
    assert_eq!(json["timings"]["store_secs"], 0.066);
    assert_eq!(json["pages_per_second"], 2.0 / 1.5);
    assert_eq!(json["items_per_second"], 2.0);
    assert_eq!(json["bytes_per_second"], 28.0);
    assert_eq!(json["bytes_downloaded"], 42);
    assert_eq!(json["error_rate"], 0.5);
    assert_eq!(json["success_rate"], 0.5);
    assert_eq!(json["retry_exhaustion_rate"], 1.0);
    assert_eq!(json["stop_reason"], "max_errors");
    assert_eq!(json["domains"]["example.com"]["scheduled"], 1);
    assert_eq!(json["domains"]["example.com"]["deduped"], 1);
    assert_eq!(json["domains"]["example.com"]["completed"], 1);
    assert_eq!(json["domains"]["example.com"]["failed"], 1);
    assert_eq!(
        json["domains"]["example.com"]["error_kinds"]["http_status"],
        1
    );
    assert_eq!(json["domains"]["example.com"]["retries"], 1);
    assert_eq!(json["retry_exhausted"], 1);
    assert_eq!(json["domains"]["example.com"]["retry_exhausted"], 1);
    assert_eq!(json["domains"]["example.com"]["robots_blocked"], 1);

    let compact: serde_json::Value = serde_json::from_str(&report.to_json_string()).unwrap();
    let pretty: serde_json::Value = serde_json::from_str(&report.to_json_string_pretty()).unwrap();
    assert_eq!(compact, json);
    assert_eq!(pretty, json);
}

#[test]
fn crawl_report_exposes_timing_breakdown() {
    let stats = CrawlStats {
        timings: CrawlTimingStats {
            middleware_request: Duration::from_millis(1),
            fetch: Duration::from_millis(2),
            middleware_response: Duration::from_millis(3),
            parse: Duration::from_millis(4),
            pipeline: Duration::from_millis(5),
            store: Duration::from_millis(6),
        },
        ..CrawlStats::default()
    };

    let report = CrawlReport::from(stats);

    assert_eq!(report.timings.middleware_request, Duration::from_millis(1));
    assert_eq!(report.timings.fetch, Duration::from_millis(2));
    assert_eq!(report.timings.middleware_response, Duration::from_millis(3));
    assert_eq!(report.timings.parse, Duration::from_millis(4));
    assert_eq!(report.timings.pipeline, Duration::from_millis(5));
    assert_eq!(report.timings.store, Duration::from_millis(6));
}

#[test]
fn crawl_report_exposes_production_rate_helpers() {
    let report = CrawlReport::from(CrawlStats {
        pages_crawled: 20,
        items_scraped: 50,
        errors: 5,
        bytes_downloaded: 10_000,
        retries: 4,
        retry_exhausted: 2,
        duration: Duration::from_secs(10),
        ..CrawlStats::default()
    });

    assert_eq!(report.pages_per_second(), 2.0);
    assert_eq!(report.items_per_second(), 5.0);
    assert_eq!(report.bytes_per_second(), 1_000.0);
    assert_eq!(report.error_rate(), 0.2);
    assert_eq!(report.success_rate(), 0.8);
    assert_eq!(report.retry_exhaustion_rate(), 0.5);
}

#[test]
fn crawl_report_rate_helpers_return_zero_when_denominator_is_zero() {
    let report = CrawlReport::from(CrawlStats {
        pages_crawled: 0,
        items_scraped: 10,
        errors: 0,
        bytes_downloaded: 10,
        retries: 0,
        retry_exhausted: 1,
        duration: Duration::ZERO,
        ..CrawlStats::default()
    });

    assert_eq!(report.pages_per_second(), 0.0);
    assert_eq!(report.items_per_second(), 0.0);
    assert_eq!(report.bytes_per_second(), 0.0);
    assert_eq!(report.error_rate(), 0.0);
    assert_eq!(report.success_rate(), 0.0);
    assert_eq!(report.retry_exhaustion_rate(), 0.0);
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

#[test]
fn record_error_kind_tracks_global_and_domain_error_breakdown() {
    let mut stats = CrawlStats::default();
    stats.record_error_kind("example.com", KumoErrorKind::Parse);
    stats.record_error_kind("example.com", KumoErrorKind::Parse);
    stats.record_error_kind("api.example.com", KumoErrorKind::HttpStatus);

    let report = CrawlReport::from(stats);

    assert_eq!(report.errors, 3);
    assert_eq!(report.error_kinds["parse"], 2);
    assert_eq!(report.error_kinds["http_status"], 1);
    assert_eq!(report.domains["example.com"].failed, 2);
    assert_eq!(report.domains["example.com"].error_kinds["parse"], 2);
    assert_eq!(report.domains["api.example.com"].failed, 1);
    assert_eq!(
        report.domains["api.example.com"].error_kinds["http_status"],
        1
    );
}

struct DuplicateSpider {
    start: String,
    target: String,
}

struct PanicSpider {
    start: String,
    name: &'static str,
}

struct SlowRequestMiddleware;

struct TimingSpider {
    start: String,
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

#[async_trait::async_trait]
impl Middleware for SlowRequestMiddleware {
    async fn before_request(&self, _request: &mut FetchRequest) -> Result<(), KumoError> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Spider for TimingSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "timing-stats"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
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

#[tokio::test]
async fn engine_stats_accumulate_successful_request_timings() {
    let start = "https://timing.example.com/start";
    let mock = MockFetcher::new().with_response(start, 200, "<h1>ok</h1>");

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .respect_robots_txt(false)
        .fetcher(mock)
        .middleware(SlowRequestMiddleware)
        .store(StdoutStore)
        .run(TimingSpider {
            start: start.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert!(stats.timings.middleware_request >= Duration::from_millis(10));
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
