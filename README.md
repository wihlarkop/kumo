# kumo

[![CI](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml/badge.svg)](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml)

<p align="center">
  <img src="assets/logo.png" alt="kumo logo" width="200">
</p>

An async web crawling framework for Rust — Scrapy for Rust.

**kumo** (蜘蛛/雲 — spider/cloud) gives you a trait-based, async-first API for writing spiders that scrape, follow links, and store data.

## Features

- **Async-first** — Tokio-based with bounded concurrency via `JoinSet`
- **CSS selectors** — `res.css(".selector")` backed by `scraper`
- **Regex selectors** — `res.re(r"\d+")`, `el.re_first(r"...")`, works on `Response`, `Element`, and `ElementList`
- **JSONPath selectors** — `res.jsonpath("$.store.books[*].title")` for JSON responses (feature: `jsonpath`)
- **Rate limiting** — token-bucket `RateLimiter` via `governor`
- **Auto-throttle** — adaptive delay based on EWMA latency and 429/503 back-off
- **Retry with backoff** — exponential backoff via `.retry(max, base_delay)`
- **robots.txt** — per-domain fetch + cache, enabled by default
- **Bloom filter dedup** — O(1) URL deduplication, 1M URLs at 0.1% false-positive rate
- **Pluggable storage** — `JsonlStore`, `JsonStore`, `StdoutStore`, PostgreSQL, SQLite, MySQL
- **Middleware chain** — `before_request` / `after_response` hooks
- **Domain + depth filtering** — `allowed_domains()` and `max_depth()` on the `Spider` trait
- **LLM extraction** — extract structured data without selectors using Claude, OpenAI, Gemini, or Ollama

## Installation

```toml
[dependencies]
kumo = "0.1"
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

## Quick Start

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
    fn name(&self) -> &str { "quotes" }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        let quotes: Vec<Quote> = res.css(".quote").iter().map(|el| Quote {
            text:   el.css(".text").first().map(|e| e.text()).unwrap_or_default(),
            author: el.css(".author").first().map(|e| e.text()).unwrap_or_default(),
        }).collect();

        let next = res.css("li.next a").first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(quotes);
        if let Some(url) = next { output = output.follow(url); }
        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    CrawlEngine::builder()
        .concurrency(5)
        .middleware(DefaultHeaders::new().user_agent("kumo/0.1"))
        .store(JsonlStore::new("quotes.jsonl"))
        .run(QuotesSpider)
        .await?;
    Ok(())
}
```

For more advanced examples — rate limiting, database stores, LLM extraction, and all selector types — see the [`examples/`](examples/) folder.

## Feature Flags

| Flag | Pulls in | Purpose |
|---|---|---|
| _(default)_ | — | CSS + regex selectors, all stores, middleware |
| `jsonpath` | `jsonpath-rust` | JSONPath selector on `Response` |
| `postgres` | `sqlx` | `PostgresStore` |
| `sqlite` | `sqlx` | `SqliteStore` |
| `mysql` | `sqlx` | `MySqlStore` |
| `claude` | `rig-core` | `AnthropicClient` for LLM extraction |
| `openai` | `rig-core` | `OpenAiClient` for LLM extraction |
| `gemini` | `rig-core` | `GeminiClient` for LLM extraction |
| `ollama` | `rig-core` | `OllamaClient` for LLM extraction |

## Roadmap

| Version | Status | Features |
|---------|--------|---------|
| v0.1 | released | Spider trait, CrawlEngine, CSS extraction, RateLimiter, AutoThrottle, retry, robots.txt, JsonlStore, JsonStore, StdoutStore |
| v0.2 | released | PostgreSQL / SQLite / MySQL stores, custom columns, LLM extraction (Claude, OpenAI, Gemini, Ollama), regex + JSONPath selectors |
| v0.3 | planned | CLI (`kumo run`), Redis frontier, headless browser fetcher, S3 store, proxy rotation, Ratatui dashboard |

## License

MIT
