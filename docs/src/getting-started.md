# Getting Started

## Prerequisites

- Rust 1.75+ (stable toolchain)
- `tokio` runtime
- `async-trait` crate

## Installation

Add kumo to your `Cargo.toml`:

```toml
[dependencies]
kumo = "0.1"
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

For optional features (database stores, browser mode, LLM extraction) see [Feature Flags](feature-flags.md).

## Your First Spider

A spider has four required parts:

1. **An item type** — a `Serialize` struct representing what you scrape
2. **`name()`** — a unique identifier for this spider
3. **`start_urls()`** — where the crawl begins
4. **`parse()`** — how to extract items and follow links from a response

```rust
use kumo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Quote {
    text: String,
    author: String,
}

struct QuotesSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    type Item = Quote;

    fn name(&self) -> &str { "quotes" }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let quotes: Vec<Quote> = res.css(".quote").iter().map(|el| Quote {
            text:   el.css(".text").first().map(|e| e.text()).unwrap_or_default(),
            author: el.css(".author").first().map(|e| e.text()).unwrap_or_default(),
        }).collect();

        // Follow pagination
        let next = res.css("li.next a").first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(quotes);
        if let Some(url) = next { output = output.follow(url); }
        Ok(output)
    }
}
```

## Running the Crawl

Use `CrawlEngine::builder()` to configure and launch:

```rust
#[tokio::main]
async fn main() -> Result<(), KumoError> {
    CrawlEngine::builder()
        .concurrency(5)                                            // parallel requests
        .middleware(DefaultHeaders::new().user_agent("kumo/0.1")) // set User-Agent
        .store(JsonlStore::new("quotes.jsonl")?)                  // write to JSONL
        .run(QuotesSpider)
        .await?;
    Ok(())
}
```

This crawls all pages, writes each `Quote` as a JSON line to `quotes.jsonl`, and exits when the frontier is empty.

## What's Next?

- [Spiders](spiders.md) — full Spider trait API, lifecycle hooks, error handling
- [Extractors](extractors.md) — CSS, XPath, Regex, JSONPath, `#[derive(Extract)]`, LLM
- [Stores](stores.md) — JSONL, JSON, CSV, PostgreSQL, SQLite, MySQL
- [Middleware](middleware.md) — rate limiting, auto-throttle, retry, proxy rotation
