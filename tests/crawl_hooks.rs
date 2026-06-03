use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use kumo::{
    CrawlHook, HookErrorPolicy,
    engine::CrawlEngine,
    error::{KumoError, KumoErrorKind},
    events::CrawlEvent,
    extract::Response,
    fetch::MockFetcher,
    spider::{Output, Spider},
};
use serde_json::json;
use tokio::sync::{Mutex, broadcast::error::TryRecvError};

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

#[derive(Clone)]
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

#[derive(Clone)]
struct RecordingHook {
    events: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl CrawlHook for RecordingHook {
    async fn on_event(&self, event: &CrawlEvent) -> Result<(), KumoError> {
        self.events.lock().await.push(event.name().to_string());
        Ok(())
    }
}

struct EmptyHook;

#[async_trait]
impl CrawlHook for EmptyHook {}

struct FailingRequestStartedHook;

#[async_trait]
impl CrawlHook for FailingRequestStartedHook {
    async fn on_request_started(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Err(KumoError::hook("boom"))
    }
}

#[tokio::test]
async fn hook_receives_crawl_lifecycle_events() {
    let url = "https://example.com/hook-lifecycle";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let events = Arc::new(Mutex::new(Vec::new()));

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(RecordingHook {
            events: events.clone(),
        })
        .run(OnePageSpider {
            name: "hook-lifecycle",
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    let events = events.lock().await;
    assert!(events.iter().any(|event| event == "crawl_started"));
    assert!(events.iter().any(|event| event == "crawl_finished"));
}

#[tokio::test]
async fn hook_receives_request_and_item_events() {
    let url = "https://example.com/hook-request";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let events = Arc::new(Mutex::new(Vec::new()));

    CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(RecordingHook {
            events: events.clone(),
        })
        .run(OnePageSpider {
            name: "hook-request",
            url: url.to_string(),
        })
        .await
        .unwrap();

    let events = events.lock().await;
    assert!(events.iter().any(|event| event == "request_scheduled"));
    assert!(events.iter().any(|event| event == "request_started"));
    assert!(events.iter().any(|event| event == "item_scraped"));
    assert!(events.iter().any(|event| event == "request_completed"));
}

#[tokio::test]
async fn hook_default_methods_are_noop() {
    let url = "https://example.com/hook-noop";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(EmptyHook)
        .run(OnePageSpider {
            name: "hook-noop",
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
}

#[tokio::test]
async fn log_and_continue_policy_ignores_hook_errors() {
    let url = "https://example.com/hook-continue";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(FailingRequestStartedHook)
        .hook_error_policy(HookErrorPolicy::LogAndContinue)
        .run(OnePageSpider {
            name: "hook-continue",
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.errors, 0);
}

#[tokio::test]
async fn abort_policy_stops_on_hook_error() {
    let url = "https://example.com/hook-abort";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");

    let err = CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(FailingRequestStartedHook)
        .hook_error_policy(HookErrorPolicy::AbortCrawl)
        .run(OnePageSpider {
            name: "hook-abort",
            url: url.to_string(),
        })
        .await
        .unwrap_err();

    assert_eq!(err.kind(), KumoErrorKind::Hook);
    assert!(err.to_string().contains("request_started"));
}

struct OrderedHook {
    id: usize,
    calls: Arc<Mutex<Vec<usize>>>,
}

#[async_trait]
impl CrawlHook for OrderedHook {
    async fn on_crawl_started(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        self.calls.lock().await.push(self.id);
        Ok(())
    }
}

#[tokio::test]
async fn hooks_run_in_registration_order() {
    let url = "https://example.com/hook-order";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let calls = Arc::new(Mutex::new(Vec::new()));

    CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(OrderedHook {
            id: 1,
            calls: calls.clone(),
        })
        .hook(OrderedHook {
            id: 2,
            calls: calls.clone(),
        })
        .run(OnePageSpider {
            name: "hook-order",
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(*calls.lock().await, vec![1, 2]);
}

#[tokio::test]
async fn hooks_and_broadcast_events_both_receive_events() {
    let url = "https://example.com/hook-channel";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let hook_events = Arc::new(Mutex::new(Vec::new()));
    let (engine, mut rx) = CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(RecordingHook {
            events: hook_events.clone(),
        })
        .event_channel(64);

    engine
        .run(OnePageSpider {
            name: "hook-channel",
            url: url.to_string(),
        })
        .await
        .unwrap();

    let hook_events = hook_events.lock().await;
    let channel_events = drain_events(&mut rx);
    assert!(hook_events.iter().any(|event| event == "request_started"));
    assert!(
        channel_events
            .iter()
            .any(|event| event.name() == "request_started")
    );
}

struct IndexHook {
    indexes: Arc<Mutex<Vec<usize>>>,
}

#[async_trait]
impl CrawlHook for IndexHook {
    async fn on_crawl_started(&self, event: &CrawlEvent) -> Result<(), KumoError> {
        if let CrawlEvent::CrawlStarted {
            spider_index: Some(index),
            ..
        } = event
        {
            self.indexes.lock().await.push(*index);
        }
        Ok(())
    }
}

#[tokio::test]
async fn hooks_work_with_run_all_spider_indexes() {
    let first = "https://example.com/hook-first";
    let second = "https://example.com/hook-second";
    let fetcher = MockFetcher::new()
        .with_response(first, 200, "<h1>first</h1>")
        .with_response(second, 200, "<h1>second</h1>");
    let indexes = Arc::new(Mutex::new(Vec::new()));

    CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(IndexHook {
            indexes: indexes.clone(),
        })
        .add_spider(OnePageSpider {
            name: "first",
            url: first.to_string(),
        })
        .add_spider(OnePageSpider {
            name: "second",
            url: second.to_string(),
        })
        .run_all()
        .await
        .unwrap();

    let mut indexes = indexes.lock().await.clone();
    indexes.sort_unstable();
    assert_eq!(indexes, vec![0, 1]);
}

struct CountingHook {
    count: Arc<AtomicUsize>,
}

#[async_trait]
impl CrawlHook for CountingHook {
    async fn on_item_scraped(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        self.count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[tokio::test]
async fn typed_lifecycle_methods_can_handle_specific_events() {
    let url = "https://example.com/hook-typed";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>ok</h1>");
    let count = Arc::new(AtomicUsize::new(0));

    CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(CountingHook {
            count: count.clone(),
        })
        .run(OnePageSpider {
            name: "hook-typed",
            url: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 1);
}
