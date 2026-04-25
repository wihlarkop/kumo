//! Demonstrates the advanced LinkExtractor API.
//! Run with: cargo run --example link_extractor

use kumo::prelude::*;

struct SiteSpider;

#[async_trait::async_trait]
impl Spider for SiteSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "link-demo"
    }
    fn start_urls(&self) -> Vec<String> {
        vec!["https://books.toscrape.com".into()]
    }
    fn max_depth(&self) -> Option<usize> {
        Some(1)
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let links = LinkExtractor::new()
            .allow_domains(&["books.toscrape.com"]) // stay on-site
            .allow(r"catalogue/") // only product pages
            .deny(r"catalogue/page") // skip pagination
            .canonicalize(true) // collapse #fragment variants
            .extract(res);

        println!("Found {} product links on {}", links.len(), res.url());

        let items = links
            .iter()
            .map(|u| serde_json::json!({"url": u}))
            .collect();
        Ok(Output::new().items(items).follow_many(links))
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();
    let stats = CrawlEngine::builder()
        .concurrency(2)
        .respect_robots_txt(true)
        .run(SiteSpider)
        .await?;
    println!("Crawled {} pages", stats.pages_crawled);
    Ok(())
}
