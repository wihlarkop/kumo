//! Scrapes quotes from https://quotes.toscrape.com using LLM extraction.
//!
//! Instead of writing CSS selectors, we derive JsonSchema on the struct and let
//! an LLM extract the fields directly from the HTML.
//!
//! Run with:
//!   ANTHROPIC_API_KEY=sk-ant-... cargo run --example llm_extract --features claude

use kumo::llm::anthropic::models;
use kumo::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Quote {
    /// The full quote text, including surrounding punctuation marks
    text: String,
    /// The author's full name as displayed on the page
    author: String,
    /// List of tag labels associated with this quote
    tags: Vec<String>,
}

struct QuotesSpider {
    client: AnthropicClient,
}

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    fn name(&self) -> &str {
        "quotes-llm"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        // No CSS selectors — the LLM reads the HTML and fills in the struct.
        let quotes: Vec<Quote> = res.extract(&self.client).await?;

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

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| {
        eprintln!("ANTHROPIC_API_KEY not set.");
        std::process::exit(1);
    });

    let client = AnthropicClient::new(api_key)
        .model(models::CLAUDE_HAIKU_4_5)
        .system_prompt("Extract all quotes from this quotes listing page. Each page contains multiple quotes in .quote elements.")
        .strip_scripts_and_styles(true);

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .middleware(DefaultHeaders::new().user_agent("kumo/0.1 (+https://github.com/wihlarkop/kumo)"))
        .store(StdoutStore)
        .run(QuotesSpider { client })
        .await?;

    println!(
        "Done — scraped {} items from {} pages ({} errors)",
        stats.items_scraped, stats.pages_crawled, stats.errors
    );
    Ok(())
}
