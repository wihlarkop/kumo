# kumo

[![CI](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml/badge.svg)](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml)

<p align="center">
  <img src="assets/logo.png" alt="kumo logo" width="200">
</p>

An async web crawling framework for Rust — Scrapy for Rust.

**kumo** (蜘蛛/雲 — spider/cloud) gives you a trait-based, async-first API for writing spiders that scrape, follow links, and store data. Batteries included: rate limiting, retry with backoff, robots.txt, and pluggable storage.

## Features

- **Async-first** — built on Tokio with a bounded `JoinSet` for controlled concurrency
- **CSS extraction** — ergonomic `res.css(".selector")` API backed by `scraper`
- **Rate limiting** — token-bucket `RateLimiter` middleware via `governor`
- **Retry with backoff** — exponential backoff via `.retry(max, base_delay)`
- **robots.txt** — per-domain fetch + cache, enabled by default
- **Bloom filter dedup** — O(1) URL deduplication in `MemoryFrontier` (1M URLs, 0.1% FP)
- **Pluggable storage** — `JsonlStore`, `JsonStore`, `StdoutStore`, or implement `ItemStore`
- **Middleware chain** — `before_request` / `after_response` hooks (inject headers, rate limit, etc.)
- **Domain filtering** — `allowed_domains()` and `max_depth()` on the `Spider` trait

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
kumo = "0.1"
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
tracing-subscriber = "0.3"
```

## Quick Start

```rust
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
    fn name(&self) -> &str { "quotes" }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        let quotes: Vec<Quote> = res.css(".quote").iter().map(|el| Quote {
            text:   el.css(".text").first().map(|e| e.text()).unwrap_or_default(),
            author: el.css(".author").first().map(|e| e.text()).unwrap_or_default(),
            tags:   el.css(".tag").iter().map(|e| e.text()).collect(),
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
    tracing_subscriber::fmt().with_env_filter("kumo=info").init();

    let stats = CrawlEngine::builder()
        .concurrency(5)
        .middleware(DefaultHeaders::new().user_agent("kumo/0.1"))
        .store(JsonlStore::new("quotes.jsonl"))
        .run(QuotesSpider)
        .await?;

    println!("Done — {} items from {} pages", stats.items_scraped, stats.pages_crawled);
    Ok(())
}
```

## Examples

```bash
# Scrape all quotes from quotes.toscrape.com (10 pages, 100 items)
cargo run --example quotes

# Scrape all books from books.toscrape.com (50 pages, 1000 items)
# Demonstrates rate limiting, retry, JsonStore
cargo run --example books
```

## Storage Backends

Kumo ships with `StdoutStore`, `JsonlStore`, and `JsonStore` out of the box. Additional backends are opt-in via feature flags.

### PostgreSQL (feature: `postgres`)

```toml
[dependencies]
kumo = { version = "0.1", features = ["postgres"] }
```

```rust
let store = PostgresStore::connect("postgres://user:pass@localhost/db").await?;

CrawlEngine::builder()
    .store(store)
    .run(MySpider)
    .await?;
```

Items land in a `kumo_items` table:

```sql
CREATE TABLE kumo_items (
    id         BIGSERIAL PRIMARY KEY,
    data       JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Query scraped data with JSONB operators
SELECT data->>'text', data->>'author' FROM kumo_items;
```

Custom table name:

```rust
let store = PostgresStore::builder("postgres://...")
    .table("my_quotes")
    .connect()
    .await?;
```

### SQLite (feature: `sqlite`)

```toml
[dependencies]
kumo = { version = "0.1", features = ["sqlite"] }
```

```rust
let store = SqliteStore::connect("sqlite://quotes.db").await?;
// or in-memory:
let store = SqliteStore::connect("sqlite::memory:").await?;

CrawlEngine::builder()
    .store(store)
    .run(MySpider)
    .await?;
```

Items are stored as JSON text in `kumo_items(id, data TEXT, created_at TEXT)`.

### MySQL / MariaDB (feature: `mysql`)

```toml
[dependencies]
kumo = { version = "0.1", features = ["mysql"] }
```

```rust
let store = MySqlStore::connect("mysql://user:pass@localhost/db").await?;

CrawlEngine::builder()
    .store(store)
    .run(MySpider)
    .await?;
```

Items are stored as native JSON in `kumo_items(id, data JSON, created_at DATETIME)`.

## Architecture

```
start_urls
    │
    ▼
MemoryFrontier (Bloom filter dedup)
    │
    ▼
Middleware chain (before_request)
    │
    ▼
HttpFetcher (reqwest)
    │
    ▼
Middleware chain (after_response)
    │
    ▼
Spider::parse(Response) → Output { items, follow }
    │              │
    ▼              ▼
ItemStore     MemoryFrontier (enqueue follow URLs)
(JsonlStore,
 JsonStore,
 StdoutStore)
```

## Implementing a Spider

```rust
#[async_trait::async_trait]
impl Spider for MySpider {
    fn name(&self) -> &str { "my-spider" }
    fn start_urls(&self) -> Vec<String> { vec!["https://example.com".into()] }

    // Optional overrides:
    fn allowed_domains(&self) -> Vec<&str> { vec!["example.com"] }
    fn max_depth(&self) -> Option<usize> { Some(10) }
    fn on_error(&self, _url: &str, _err: &KumoError) -> ErrorPolicy { ErrorPolicy::Skip }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        // res.css(), res.text(), res.json(), res.urljoin()
        Ok(Output::new())
    }
}
```

## Engine Builder

```rust
CrawlEngine::builder()
    .concurrency(8)
    .middleware(RateLimiter::per_second(5.0))
    .middleware(DefaultHeaders::new().user_agent("my-bot/1.0"))
    .store(JsonlStore::new("output.jsonl"))
    .crawl_delay(Duration::from_millis(200))
    .retry(3, Duration::from_millis(500))
    .respect_robots_txt(true)
    .run(MySpider)
    .await?;
```

## Roadmap

| Version | Features |
|---------|---------|
| v0.1 (current) | Spider trait, CrawlEngine, CSS extraction, RateLimiter, retry, robots.txt, JsonlStore, JsonStore |
| v0.2 | Redis frontier, headless browser fetcher, PostgreSQL store, S3 store, auto-throttle, proxy rotation |
| v0.3 | CLI (`kumo run`), LLM-based extraction (Claude API), Ratatui dashboard, plugin system |

## License

MIT
