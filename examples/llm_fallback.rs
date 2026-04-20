//! Demonstrates LLM fallback extraction via `#[extract(llm_fallback = "...")]`.
//!
//! The `price` field has an intentionally broken CSS selector so it always
//! returns empty — the LLM then fills it in from the page HTML.
//!
//! Run with:
//!   ANTHROPIC_API_KEY=sk-ant-... cargo run --example llm_fallback --features derive,claude

use kumo::prelude::*;
use serde::Serialize;
use std::sync::Arc;

#[derive(ExtractDerive, Serialize, Debug)]
struct Book {
    #[extract(css = "h3 a", attr = "title")]
    title: String,

    // Intentionally broken selector — LLM will extract the price instead.
    #[extract(
        css = ".no-such-class",
        llm_fallback = "the book price including currency symbol"
    )]
    price: String,

    #[extract(css = ".star-rating", attr = "class")]
    rating: Option<String>,
}

struct BooksSpider {
    client: Arc<AnthropicClient>,
}

#[async_trait::async_trait]
impl Spider for BooksSpider {
    type Item = Book;

    fn name(&self) -> &str {
        "books-llm-fallback"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://books.toscrape.com/catalogue/page-1.html".into()]
    }

    fn allowed_domains(&self) -> Vec<&str> {
        vec!["books.toscrape.com"]
    }

    fn max_depth(&self) -> Option<usize> {
        Some(1)
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let mut books = Vec::new();
        for el in res.css("article.product_pod").iter() {
            books.push(Book::extract_from(el, Some(self.client.as_ref())).await?);
        }
        Ok(Output::new().items(books))
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

    let client = Arc::new(
        AnthropicClient::new(api_key)
            .system_prompt("Extract book information from this product card HTML."),
    );

    let stats = CrawlEngine::builder()
        .concurrency(1)
        .store(StdoutStore)
        .run(BooksSpider {
            client: Arc::clone(&client),
        })
        .await?;

    let usage = client.total_usage();
    println!(
        "Done — {} items from {} pages",
        stats.items_scraped, stats.pages_crawled
    );
    println!(
        "LLM tokens — {} in / {} out ({} cached)",
        usage.input_tokens, usage.output_tokens, usage.cached_input_tokens
    );
    Ok(())
}
