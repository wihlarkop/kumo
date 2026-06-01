use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::MockFetcher,
    pipeline::FilterPipeline,
    spider::{Output, Spider},
};
use tracing::{Event, Subscriber, field};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

#[derive(Clone, Debug, Default)]
struct CapturedEvent {
    target: String,
    fields: BTreeMap<String, String>,
}

#[derive(Default)]
struct FieldVisitor {
    fields: BTreeMap<String, String>,
}

impl field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &field::Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

struct CaptureLayer {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        self.events.lock().unwrap().push(CapturedEvent {
            target: event.metadata().target().to_string(),
            fields: visitor.fields,
        });
    }
}

struct LoggingSpider {
    start: String,
}

#[async_trait::async_trait]
impl Spider for LoggingSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "logging-contract"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start.clone()]
    }

    async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new().item(serde_json::json!({ "title": "drop me" })))
    }
}

fn captured_value<'a>(events: &'a [CapturedEvent], event: &str, field: &str) -> Option<&'a str> {
    events
        .iter()
        .find(|entry| {
            entry
                .fields
                .get("event")
                .is_some_and(|value| value == event)
        })
        .and_then(|entry| entry.fields.get(field))
        .map(String::as_str)
}

#[tokio::test(flavor = "current_thread")]
async fn crawl_logs_include_stable_event_fields() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer {
        events: events.clone(),
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    let url = "https://example.com";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>Hello</h1>");

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .respect_robots_txt(false)
        .pipeline(FilterPipeline::new(|_| false))
        .run(LoggingSpider {
            start: url.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 0);

    let events = events.lock().unwrap().clone();

    assert!(events.iter().any(|entry| {
        entry.target == "kumo::crawl"
            && entry
                .fields
                .get("event")
                .is_some_and(|v| v == "crawl.start")
            && entry
                .fields
                .get("spider")
                .is_some_and(|v| v == "logging-contract")
    }));
    assert!(events.iter().any(|entry| {
        entry.target == "kumo::crawl"
            && entry
                .fields
                .get("event")
                .is_some_and(|v| v == "crawl.complete")
            && entry.fields.get("pages").is_some_and(|v| v == "1")
            && entry.fields.get("items").is_some_and(|v| v == "0")
    }));
    assert_eq!(
        captured_value(&events, "request.ok", "url"),
        Some("https://example.com")
    );
    assert_eq!(captured_value(&events, "request.ok", "depth"), Some("0"));
    assert_eq!(captured_value(&events, "request.ok", "items"), Some("0"));
    assert_eq!(
        captured_value(&events, "item.drop", "reason"),
        Some("filter")
    );
}
