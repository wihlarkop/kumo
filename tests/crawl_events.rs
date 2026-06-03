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
