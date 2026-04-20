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
- **XPath selectors** — `res.xpath("//h1/text()")` for XML/HTML documents (feature: `xpath`)
- **Regex selectors** — `res.re(r"\d+")`, `el.re_first(r"...")`, works on `Response`, `Element`, and `ElementList`
- **JSONPath selectors** — `res.jsonpath("$.store.books[*].title")` for JSON responses (feature: `jsonpath`)
- **`#[derive(Extract)]`** — generate CSS extraction boilerplate from field annotations (feature: `derive`)
- **LLM adaptive extraction** — `#[extract(llm_fallback = "hint")]` falls back to an LLM when a selector returns empty (features: `derive` + any LLM provider)
- **Rate limiting** — token-bucket `RateLimiter` via `governor`
- **Auto-throttle** — adaptive delay based on EWMA latency and 429/503 back-off
- **Retry with backoff** — exponential backoff via `.retry(max, base_delay)`
- **robots.txt** — per-domain fetch + cache, enabled by default
- **Bloom filter dedup** — O(1) URL deduplication, 1M URLs at 0.1% false-positive rate
- **HTTP cache** — disk-backed response cache via `.http_cache(dir)`, optional TTL
- **Link extractor** — `LinkExtractor` middleware auto-enqueues `<a href>` links
- **Pluggable storage** — `JsonlStore`, `JsonStore`, `CsvStore`, `StdoutStore`, PostgreSQL, SQLite, MySQL
- **Middleware chain** — `before_request` / `after_response` hooks, proxy rotation, custom headers
- **Domain + depth filtering** — `allowed_domains()` and `max_depth()` on the `Spider` trait
- **Multi-spider engine** — run multiple spiders concurrently via `.add_spider()` / `.run_all()`
- **LLM extraction** — extract structured data without selectors using Claude, OpenAI, Gemini, or Ollama
- **Browser fetcher** — headless Chromium via chromiumoxide for JS-rendered pages (feature: `browser`)
- **Stealth mode** — JS fingerprint patches for browser + TLS/HTTP2 fingerprint spoofing via `rquest` (feature: `stealth`)
- **Distributed frontier** — Redis-backed URL frontier for multi-process crawls (feature: `redis-frontier`)
- **Persistent frontier** — file-backed URL frontier that survives restarts (feature: `persistence`)
- **Metrics** — periodic stats snapshots via `tracing::info!` using `.metrics_interval()`

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
        .store(JsonlStore::new("quotes.jsonl")?)
        .run(QuotesSpider)
        .await?;
    Ok(())
}
```

For more examples — rate limiting, database stores, LLM extraction, browser mode, stealth, and all selector types — see the [`examples/`](examples/) folder.

## Feature Flags

| Flag | Pulls in | Purpose |
|---|---|---|
| _(default)_ | — | CSS + regex selectors, all stores, middleware, HTTP cache, link extractor |
| `derive` | `kumo-derive` | `#[derive(Extract)]` for zero-boilerplate CSS extraction |
| `jsonpath` | `jsonpath-rust` | JSONPath selector on `Response` |
| `xpath` | `sxd-xpath` | XPath selector on `Response` |
| `browser` | `chromiumoxide` | Headless Chromium fetcher for JS-rendered pages |
| `stealth` | `rquest`, `rquest-util` | TLS/HTTP2 fingerprint spoofing + browser stealth patches¹ |
| `claude` | `rig-core` | `AnthropicClient` for LLM extraction / fallback |
| `openai` | `rig-core` | `OpenAiClient` for LLM extraction / fallback |
| `gemini` | `rig-core` | `GeminiClient` for LLM extraction / fallback |
| `ollama` | `rig-core` | `OllamaClient` for local LLM extraction / fallback |
| `llm` | `rig-core`, `schemars` | Base LLM traits (implied by all provider flags) |
| `postgres` | `sqlx` | `PostgresStore` |
| `sqlite` | `sqlx` | `SqliteStore` |
| `mysql` | `sqlx` | `MySqlStore` |
| `persistence` | — | `FileFrontier` — file-backed URL frontier that survives restarts |
| `redis-frontier` | `redis` | `RedisFrontier` — distributed URL frontier via Redis |

> ¹ The `stealth` feature compiles BoringSSL from source. It requires **cmake** and **nasm** on the build machine (`apt install cmake nasm` on Ubuntu, `brew install cmake nasm` on macOS).

## LLM Adaptive Extraction

Combine `#[derive(Extract)]` with `llm_fallback` to gracefully handle pages where selectors sometimes return nothing:

```rust
#[derive(Extract, Serialize)]
struct Product {
    #[extract(css = "h1.title")]
    title: String,

    // Falls back to LLM when the selector returns empty
    #[extract(css = ".price", llm_fallback = "the product price including currency symbol")]
    price: String,

    #[extract(css = ".stock", llm_fallback)]  // uses field name as the hint
    stock: Option<String>,
}

// Without LLM (selector-only):
let item = Product::extract_from(&el, None).await?;

// With LLM fallback (Claude):
let item = Product::extract_from(&el, Some(&claude_client)).await?;
```

## Stealth Mode

Avoid bot detection with two complementary layers:

```rust
// Layer 1 — Browser: JS patches hide navigator.webdriver, spoof canvas/WebGL, etc.
let cfg = BrowserConfig::headless().stealth();
CrawlEngine::builder().browser(cfg).run(MySpider).await?;

// Layer 2 — HTTP: TLS + HTTP/2 fingerprint matches a real Chrome 131 client hello
// (requires the `stealth` feature with cmake/nasm build tools)
CrawlEngine::builder()
    .stealth(StealthProfile::Chrome131)
    .run(MySpider)
    .await?;

// Also available: custom reqwest client configuration (no extra deps)
CrawlEngine::builder()
    .http_client_builder(|b| b.timeout(Duration::from_secs(10)))
    .run(MySpider)
    .await?;
```

## Roadmap

| Version | Status | Features |
|---------|--------|---------|
| v0.1 | released | Spider trait, CrawlEngine, CSS extraction, RateLimiter, AutoThrottle, retry, robots.txt, JsonlStore, JsonStore, StdoutStore |
| v0.2 | released | PostgreSQL / SQLite / MySQL / CSV stores, LLM extraction (Claude, OpenAI, Gemini, Ollama), regex + JSONPath + XPath selectors, browser fetcher, HTTP cache, link extractor, multi-spider, Redis frontier, persistent frontier, metrics, `#[derive(Extract)]`, LLM adaptive extraction, stealth mode |
| v0.3 | planned | Item stream API (`CrawlEngine::stream()`), OpenTelemetry metrics, cloud storage backends (S3, GCS, Azure Blob) |

## License

MIT
