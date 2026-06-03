use async_trait::async_trait;
use kumo::{
    engine::CrawlEngine,
    error::{ErrorPolicy, KumoError},
    events::{CrawlEvent, ItemDropReason, RequestSkipReason},
    extract::Response,
    fetch::MockFetcher,
    pipeline::Pipeline,
    spider::{Output, Spider},
};
use serde_json::json;
use tokio::sync::broadcast::error::TryRecvError;
use tokio_stream::StreamExt;

fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<CrawlEvent>) -> Vec<CrawlEvent> {
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }
    events
}

#[test]
fn event_channel_capacity_is_clamped_to_one() {
    let (_engine, _rx) = CrawlEngine::builder().event_channel(0);
}

#[test]
fn events_accepts_caller_owned_sender() {
    let (tx, _rx) = tokio::sync::broadcast::channel::<CrawlEvent>(8);
    let _engine = CrawlEngine::builder().events(tx);
}

#[test]
fn crawl_event_names_are_stable() {
    use kumo::stats::{CrawlReport, CrawlStats};

    let report = CrawlReport::from(CrawlStats::default());
    let events = [
        (
            CrawlEvent::CrawlStarted {
                spider: "s".into(),
                spider_index: None,
                start_urls: 1,
            },
            "crawl_started",
        ),
        (
            CrawlEvent::RequestScheduled {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                domain: "example.com".into(),
                depth: 0,
            },
            "request_scheduled",
        ),
        (
            CrawlEvent::RequestSkipped {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                domain: "example.com".into(),
                depth: 0,
                reason: RequestSkipReason::Duplicate,
            },
            "request_skipped",
        ),
        (
            CrawlEvent::RequestStarted {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                domain: "example.com".into(),
                depth: 0,
                attempt: 0,
            },
            "request_started",
        ),
        (
            CrawlEvent::RequestCompleted {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                domain: "example.com".into(),
                depth: 0,
                attempt: 0,
                status: 200,
                bytes: 0,
                items: 0,
                elapsed: std::time::Duration::ZERO,
            },
            "request_completed",
        ),
        (
            CrawlEvent::RequestRetried {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                domain: "example.com".into(),
                depth: 0,
                attempt: 1,
                max_attempts: 1,
                delay: std::time::Duration::ZERO,
                error_kind: kumo::error::KumoErrorKind::Parse,
            },
            "request_retried",
        ),
        (
            CrawlEvent::RequestFailed {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                domain: "example.com".into(),
                depth: 0,
                attempt: 0,
                error_kind: kumo::error::KumoErrorKind::Parse,
                retry_exhausted: false,
            },
            "request_failed",
        ),
        (
            CrawlEvent::TaskPanicked {
                spider: "s".into(),
                spider_index: None,
                url: None,
                domain: None,
                depth: None,
            },
            "task_panicked",
        ),
        (
            CrawlEvent::ItemScraped {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                depth: 0,
            },
            "item_scraped",
        ),
        (
            CrawlEvent::ItemDropped {
                spider: "s".into(),
                spider_index: None,
                url: "https://example.com".into(),
                depth: 0,
                reason: ItemDropReason::PipelineFiltered,
                error_kind: None,
            },
            "item_dropped",
        ),
        (
            CrawlEvent::CrawlFinished {
                spider: "s".into(),
                spider_index: None,
                report,
                stop_reason: None,
            },
            "crawl_finished",
        ),
    ];

    for (event, expected) in events {
        assert_eq!(event.name(), expected);
    }
}

struct OnePageSpider {
    name: &'static str,
    url: String,
}

#[async_trait]
impl Spider for OnePageSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        self.name
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.url.clone()]
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new().item(json!({ "url": response.url() })))
    }
}

#[tokio::test]
async fn request_success_emits_task_events() {
    let url = "https://example.com/success";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let (engine, mut rx) = CrawlEngine::builder().fetcher(fetcher).event_channel(64);

    let stats = engine
        .run(OnePageSpider {
            name: "success",
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::RequestStarted {
            url: event_url,
            depth: 0,
            attempt: 0,
            ..
        } if event_url == url
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::ItemScraped { url: event_url, .. } if event_url == url
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::RequestCompleted {
            url: event_url,
            status: 200,
            items: 1,
            ..
        } if event_url == url
    )));
}

#[tokio::test]
async fn crawl_started_and_finished_events_are_emitted() {
    let url = "https://example.com/finish";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let (engine, mut rx) = CrawlEngine::builder().fetcher(fetcher).event_channel(64);

    engine
        .run(OnePageSpider {
            name: "finish",
            url: url.to_string(),
        })
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::CrawlStarted {
            spider,
            spider_index: None,
            start_urls: 1,
        } if spider == "finish"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::CrawlFinished {
            spider,
            spider_index: None,
            report,
            ..
        } if spider == "finish" && report.pages_crawled == 1
    )));
}

struct DuplicateFollowSpider {
    url: String,
}

#[async_trait]
impl Spider for DuplicateFollowSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "duplicate-follow"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.url.clone()]
    }

    async fn parse(&self, _response: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new().follow(self.url.clone()))
    }
}

#[tokio::test]
async fn duplicate_follow_request_emits_duplicate_skip() {
    let url = "https://example.com/duplicate";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let (engine, mut rx) = CrawlEngine::builder().fetcher(fetcher).event_channel(64);

    engine
        .run(DuplicateFollowSpider {
            url: url.to_string(),
        })
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::RequestSkipped {
            url: event_url,
            reason: RequestSkipReason::Duplicate,
            ..
        } if event_url == url
    )));
}

struct DepthLimitSpider {
    root: String,
    follow: String,
}

#[async_trait]
impl Spider for DepthLimitSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "depth-limit"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.root.clone()]
    }

    fn max_depth(&self) -> Option<usize> {
        Some(0)
    }

    async fn parse(&self, _response: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new().follow(self.follow.clone()))
    }
}

#[tokio::test]
async fn max_depth_follow_request_emits_depth_limit_skip() {
    let root = "https://example.com/root";
    let follow = "https://example.com/follow";
    let fetcher = MockFetcher::new().with_response(root, 200, "<h1>ok</h1>");
    let (engine, mut rx) = CrawlEngine::builder().fetcher(fetcher).event_channel(64);

    engine
        .run(DepthLimitSpider {
            root: root.to_string(),
            follow: follow.to_string(),
        })
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::RequestSkipped {
            url,
            depth: 1,
            reason: RequestSkipReason::DepthLimit,
            ..
        } if url == follow
    )));
}

struct ParseErrorSpider {
    url: String,
    policy: ErrorPolicy,
}

#[async_trait]
impl Spider for ParseErrorSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "parse-error"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.url.clone()]
    }

    async fn parse(&self, _response: &Response) -> Result<Output<Self::Item>, KumoError> {
        Err(KumoError::parse_msg("bad parse"))
    }

    fn on_error(&self, _url: &str, _err: &KumoError) -> ErrorPolicy {
        self.policy.clone()
    }
}

#[tokio::test]
async fn retry_policy_emits_retry_event() {
    let url = "https://example.com/retry";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>bad</h1>");
    let (engine, mut rx) = CrawlEngine::builder().fetcher(fetcher).event_channel(64);

    engine
        .run(ParseErrorSpider {
            url: url.to_string(),
            policy: ErrorPolicy::Retry(1),
        })
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::RequestRetried {
            url: event_url,
            attempt: 1,
            max_attempts: 1,
            ..
        } if event_url == url
    )));
}

#[tokio::test]
async fn permanent_failure_emits_failed_event() {
    let url = "https://example.com/fail";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>bad</h1>");
    let (engine, mut rx) = CrawlEngine::builder().fetcher(fetcher).event_channel(64);

    engine
        .run(ParseErrorSpider {
            url: url.to_string(),
            policy: ErrorPolicy::Skip,
        })
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::RequestFailed {
            url: event_url,
            error_kind,
            retry_exhausted: false,
            ..
        } if event_url == url && error_kind.as_str() == "parse"
    )));
}

struct DropAllPipeline;

#[async_trait]
impl Pipeline for DropAllPipeline {
    async fn process(
        &self,
        _item: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, KumoError> {
        Ok(None)
    }
}

#[tokio::test]
async fn pipeline_drop_emits_item_dropped_event() {
    let url = "https://example.com/drop";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let (engine, mut rx) = CrawlEngine::builder()
        .fetcher(fetcher)
        .pipeline(DropAllPipeline)
        .event_channel(64);

    engine
        .run(OnePageSpider {
            name: "drop",
            url: url.to_string(),
        })
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::ItemDropped {
            url: event_url,
            reason: ItemDropReason::PipelineFiltered,
            ..
        } if event_url == url
    )));
}

#[tokio::test]
async fn event_delivery_does_not_require_receiver() {
    let url = "https://example.com/no-receiver";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let (engine, rx) = CrawlEngine::builder().fetcher(fetcher).event_channel(4);
    drop(rx);

    let stats = engine
        .run(OnePageSpider {
            name: "no-receiver",
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
}

struct PanicSpider {
    url: String,
}

#[async_trait]
impl Spider for PanicSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "panic-events"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.url.clone()]
    }

    async fn parse(&self, _response: &Response) -> Result<Output<Self::Item>, KumoError> {
        panic!("intentional panic for event coverage");
    }
}

#[tokio::test]
async fn task_panic_emits_task_panicked_event() {
    let url = "https://panic.example.com/start";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>panic</h1>");
    let (engine, mut rx) = CrawlEngine::builder()
        .respect_robots_txt(false)
        .fetcher(fetcher)
        .event_channel(64);

    let stats = engine
        .run(PanicSpider {
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.errors, 1);
    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::TaskPanicked {
            spider,
            spider_index: None,
            url: Some(event_url),
            domain: Some(domain),
            depth: Some(0),
        } if spider == "panic-events" && event_url == url && domain == "panic.example.com"
    )));
}

struct EndlessStreamSpider {
    url: String,
}

#[async_trait]
impl Spider for EndlessStreamSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "event-stream"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.url.clone()]
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        let current = response
            .url()
            .rsplit('/')
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or_default();
        Ok(Output::new()
            .item(json!({ "page": current }))
            .follow(format!("https://example.com/page/{}", current + 1)))
    }
}

#[tokio::test]
async fn stream_cancellation_emits_finished_event_with_interrupted_stop_reason() {
    let (tx, mut rx) = tokio::sync::broadcast::channel::<CrawlEvent>(128);
    let mut stream = CrawlEngine::builder()
        .concurrency(1)
        .stream_buffer(1)
        .respect_robots_txt(false)
        .fetcher(MockFetcher::new().with_default(200, "<h1>ok</h1>"))
        .events(tx)
        .stream(EndlessStreamSpider {
            url: "https://example.com/page/0".to_string(),
        })
        .await
        .unwrap();

    let item = stream.next().await.expect("stream should yield first item");
    assert_eq!(item["page"], 0);
    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::CrawlFinished {
            spider,
            stop_reason: Some(kumo::stats::StopReason::Interrupted),
            report,
            ..
        } if spider == "event-stream" && report.interrupted
    )));
}

#[tokio::test]
async fn run_all_events_include_spider_index() {
    let url_a = "https://example.com/a";
    let url_b = "https://example.com/b";
    let fetcher = MockFetcher::new()
        .with_response(url_a, 200, "<h1>a</h1>")
        .with_response(url_b, 200, "<h1>b</h1>");
    let (engine, mut rx) = CrawlEngine::builder()
        .fetcher(fetcher)
        .add_spider(OnePageSpider {
            name: "a",
            url: url_a.to_string(),
        })
        .add_spider(OnePageSpider {
            name: "b",
            url: url_b.to_string(),
        })
        .event_channel(128);

    let stats = engine.run_all().await.unwrap();
    assert_eq!(stats.len(), 2);

    let events = drain_events(&mut rx);
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::CrawlStarted {
            spider,
            spider_index: Some(0),
            ..
        } if spider == "a"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CrawlEvent::CrawlFinished {
            spider,
            spider_index: Some(1),
            ..
        } if spider == "b"
    )));
}
