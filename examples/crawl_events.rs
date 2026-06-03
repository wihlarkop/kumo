//! Demonstrates typed crawl lifecycle events.
//!
//! Run with:
//! cargo run --example crawl_events

use kumo::prelude::*;
use serde_json::json;

struct DemoSpider {
    url: String,
}

#[async_trait::async_trait]
impl Spider for DemoSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "crawl-events-demo"
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
    let fetcher = MockFetcher::new().with_response(url, 200, "<h1>Hello from events</h1>");
    let (engine, mut events) = CrawlEngine::builder().fetcher(fetcher).event_channel(128);

    let listener = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                CrawlEvent::RequestCompleted {
                    url, status, items, ..
                } => {
                    println!("{url} status={status} items={items}");
                }
                CrawlEvent::CrawlFinished { report, .. } => {
                    println!(
                        "finished pages={} items={}",
                        report.pages_crawled, report.items_scraped
                    );
                    break;
                }
                _ => {}
            }
        }
    });

    engine
        .run(DemoSpider {
            url: url.to_string(),
        })
        .await?;
    listener.await?;
    Ok(())
}
