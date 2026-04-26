//! Scrapes all quotes from https://quotes.toscrape.com and writes them to a
//! local directory via the backend-agnostic CloudStore.
//!
//! Run with:
//!   cargo run --example cloud --features cloud
//!
//! Output: <temp_dir>/kumo-cloud-demo/quotes/items-<millis>.jsonl
//!
//! Swap `LocalFileSystem` for any `object_store` backend (S3, GCS, Azure) with
//! zero changes to the kumo code — just provide a different `Arc<dyn ObjectStore>`.
//! For S3, enable `--features cloud-s3` and build an `AmazonS3` store instead.

use std::sync::Arc;

use object_store::local::LocalFileSystem;
use serde::Serialize;

use kumo::prelude::*;
use kumo::store::CloudStore;

#[derive(Debug, Serialize)]
struct Quote {
    text: String,
    author: String,
    tags: Vec<String>,
}

struct QuotesSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    type Item = Quote;

    fn name(&self) -> &str {
        "quotes"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let quotes: Vec<Quote> = res
            .css(".quote")
            .iter()
            .map(|el| Quote {
                text: el
                    .css(".text")
                    .first()
                    .map(|e| e.text())
                    .unwrap_or_default(),
                author: el
                    .css(".author")
                    .first()
                    .map(|e| e.text())
                    .unwrap_or_default(),
                tags: el.css(".tag").iter().map(|e| e.text()).collect(),
            })
            .collect();

        let next_url = res
            .css("li.next a")
            .first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(quotes);
        if let Some(url) = next_url {
            output = output.follow(url);
        }
        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();

    let out_dir = std::env::temp_dir().join("kumo-cloud-demo");
    std::fs::create_dir_all(&out_dir).expect("create output directory");

    let backend =
        Arc::new(LocalFileSystem::new_with_prefix(&out_dir).expect("create LocalFileSystem"));

    // Swap `backend` for any object_store implementation to target S3, GCS, etc.
    let store = CloudStore::builder(backend).prefix("quotes").build();

    let stats = CrawlEngine::builder()
        .concurrency(5)
        .middleware(
            DefaultHeaders::new().user_agent("kumo/0.1 (+https://github.com/wihlarkop/kumo)"),
        )
        .store(store)
        .run(QuotesSpider)
        .await?;

    println!(
        "Done — scraped {} items from {} pages ({} errors)",
        stats.items_scraped, stats.pages_crawled, stats.errors
    );
    println!("Output: {}/quotes/items-*.jsonl", out_dir.display());
    Ok(())
}
