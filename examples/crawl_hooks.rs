//! Demonstrates crawl lifecycle hooks.
//!
//! Run with:
//! cargo run --example crawl_hooks

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use kumo::prelude::*;
use serde_json::json;

#[derive(Default, Clone)]
struct StatsHook {
    completed: Arc<AtomicUsize>,
    items: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl CrawlHook for StatsHook {
    async fn on_request_completed(&self, event: &CrawlEvent) -> Result<(), KumoError> {
        if let CrawlEvent::RequestCompleted { url, status, .. } = event {
            self.completed.fetch_add(1, Ordering::Relaxed);
            println!("completed {url} status={status}");
        }
        Ok(())
    }

    async fn on_item_scraped(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        self.items.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn on_crawl_finished(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        println!(
            "hook totals completed={} items={}",
            self.completed.load(Ordering::Relaxed),
            self.items.load(Ordering::Relaxed)
        );
        Ok(())
    }
}

struct DemoSpider {
    url: String,
}

#[async_trait::async_trait]
impl Spider for DemoSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "crawl-hooks-demo"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.url.clone()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let title = res
            .css("h1")
            .first()
            .map(|node| node.text())
            .unwrap_or_default();
        Ok(Output::new().item(json!({
            "url": res.url(),
            "title": title,
        })))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "https://example.com";
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>Hello from hooks</h1>");

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .hook(StatsHook::default())
        .hook_error_policy(HookErrorPolicy::LogAndContinue)
        .run(DemoSpider {
            url: url.to_string(),
        })
        .await?;

    println!(
        "crawl totals pages={} items={}",
        stats.pages_crawled, stats.items_scraped
    );
    Ok(())
}
