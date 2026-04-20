//! Demonstrates HTTP response caching to avoid redundant network requests.
//!
//! First run: fetches from the network and caches to ./cache/
//! Second run: served from disk — no HTTP requests made.
//!
//! Run with: cargo run --example http_cache

use kumo::prelude::*;

struct QuotesSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "quotes-cached"
    }
    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }
    fn max_depth(&self) -> Option<usize> {
        Some(1)
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let quotes: Vec<_> = res
            .css(".quote .text")
            .iter()
            .map(|e| serde_json::json!({"text": e.text()}))
            .collect();
        Ok(Output::new().items(quotes))
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();

    let stats = CrawlEngine::builder()
        .concurrency(2)
        .http_cache("./cache")? // cache responses to disk
        .cache_ttl(std::time::Duration::from_secs(3600)) // expire after 1h
        .store(StdoutStore)
        .run(QuotesSpider)
        .await?;

    println!(
        "Scraped {} quotes from {} pages",
        stats.items_scraped, stats.pages_crawled
    );
    Ok(())
}
