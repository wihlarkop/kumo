//! Scrapes all quotes from https://quotes.toscrape.com using AutoThrottle.
//!
//! Run with:
//!   cargo run --example autothrottle
//!
//! Watch the logs to see the delay adapting in real time:
//!   RUST_LOG=kumo=debug cargo run --example autothrottle

use kumo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Quote {
    text: String,
    author: String,
    tags: Vec<String>,
}

struct QuotesSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    fn name(&self) -> &str {
        "quotes-autothrottle"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
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

        let mut output = Output::new().items(quotes)?;
        if let Some(url) = next_url {
            output = output.follow(url);
        }
        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=debug")
        .init();

    let stats = CrawlEngine::builder()
        .concurrency(2)
        .middleware(
            AutoThrottle::new()
                .start_delay(std::time::Duration::from_millis(500))
                .min_delay(std::time::Duration::from_millis(100))
                .max_delay(std::time::Duration::from_secs(30)),
        )
        .middleware(
            DefaultHeaders::new().user_agent("kumo/0.1 (+https://github.com/wihlarkop/kumo)"),
        )
        .store(StdoutStore)
        .run(QuotesSpider)
        .await?;

    println!(
        "Done — scraped {} items from {} pages ({} errors)",
        stats.items_scraped, stats.pages_crawled, stats.errors
    );
    Ok(())
}
