//! Two spiders sharing one engine via run_all().
//! Run with: cargo run --example multi_spider

use kumo::prelude::*;

struct QuotesSpider;
struct HttpbinSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    type Item = serde_json::Value;
    fn name(&self) -> &str {
        "quotes"
    }
    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }
    fn max_depth(&self) -> Option<usize> {
        Some(1)
    }
    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let texts: Vec<serde_json::Value> = res
            .css(".quote .text")
            .iter()
            .map(|e| serde_json::json!({"text": e.text()}))
            .collect();
        Ok(Output::new().items(texts))
    }
}

#[async_trait::async_trait]
impl Spider for HttpbinSpider {
    type Item = serde_json::Value;
    fn name(&self) -> &str {
        "httpbin"
    }
    fn start_urls(&self) -> Vec<String> {
        vec!["https://httpbin.org/json".into()]
    }
    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let val = res.json::<serde_json::Value>()?;
        Ok(Output::new().item(val))
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();

    let all_stats = CrawlEngine::builder()
        .concurrency(4)
        .respect_robots_txt(false)
        .add_spider(QuotesSpider)
        .add_spider(HttpbinSpider)
        .run_all()
        .await?;

    for (i, stats) in all_stats.iter().enumerate() {
        println!(
            "spider {i}: {} pages, {} items, {} errors",
            stats.pages_crawled, stats.items_scraped, stats.errors
        );
    }
    Ok(())
}
