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
- **Rate limiting** — token-bucket `RateLimiter` via `governor`
- **Auto-throttle** — adaptive delay based on EWMA latency and 429/503 back-off
- **Retry with backoff** — exponential backoff via `.retry(max, base_delay)`
- **Item stream** — `CrawlEngine::stream()` returns an async `Stream` for real-time item consumption with natural backpressure
- **robots.txt** — per-domain fetch + cache, enabled by default
- **Bloom filter dedup** — O(1) URL deduplication, 1M URLs at 0.1% false-positive rate
- **HTTP cache** — disk-backed response cache via `.http_cache(dir)`, optional TTL
- **Link extractor** — `LinkExtractor` with allow/deny regex, `allow_domains`, `canonicalize`, `restrict_css`, and configurable tags/attrs
- **Pluggable storage** — `JsonlStore`, `JsonStore`, `CsvStore`, `StdoutStore`, PostgreSQL, SQLite, MySQL
- **Middleware chain** — `before_request` / `after_response` hooks, proxy rotation, custom headers
- **Domain + depth filtering** — `allowed_domains()` and `max_depth()` on the `Spider` trait
- **Multi-spider engine** — run multiple spiders concurrently via `.add_spider()` / `.run_all()`
- **LLM extraction** — extract structured data without selectors using Claude, OpenAI, Gemini, or Ollama
- **Browser fetcher** — headless Chromium via chromiumoxide for JS-rendered pages (feature: `browser`)
- **Distributed frontier** — Redis-backed URL frontier for multi-process crawls (feature: `redis-frontier`)
- **Persistent frontier** — file-backed URL frontier that survives restarts (feature: `persistence`)
- **Sitemap spider** — `SitemapSpider` reads `sitemap.xml` / sitemap index files, emits `SitemapEntry` items with `lastmod`, `priority`, `changefreq`; supports URL filtering and robots.txt autodiscovery
- **Metrics** — periodic stats snapshots via `tracing::info!` using `.metrics_interval()`
- **OpenTelemetry** — OTLP/gRPC export of all spans to Jaeger, Grafana Tempo, Datadog, etc. (feature: `otel`)

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

For more examples — rate limiting, database stores, LLM extraction, browser mode, and all selector types — see the [`examples/`](examples/) folder.

### Item Stream API

`CrawlEngine::stream()` yields items in real time as they are scraped:

```rust
use kumo::prelude::*;

let mut stream = CrawlEngine::builder()
    .concurrency(4)
    .stream(MySpider)
    .await?;

while let Some(item) = stream.next().await {
    // process each item as it arrives — no waiting for the crawl to finish
    println!("{}", item);
}
```

The crawl runs in a background Tokio task. Dropping the stream stops the crawl gracefully. Adjust backpressure with `.stream_buffer(n)` (default: 100 items):

```rust
// Slow consumer — smaller buffer pauses the crawler sooner
CrawlEngine::builder()
    .stream_buffer(10)
    .stream(MySpider)
    .await?;
```

### OpenTelemetry

Enable the `otel` feature to export all kumo spans and events to any OpenTelemetry-compatible collector (Jaeger, Grafana Tempo, Datadog, Honeycomb, etc.):

```toml
[dependencies]
kumo = { version = "0.1", features = ["otel"] }
```

Call `kumo::otel::init()` once at startup — every request, item scrape, retry, and pipeline drop is automatically traced with structured fields:

```rust
#[tokio::main]
async fn main() -> Result<(), kumo::error::KumoError> {
    // Export to a local Jaeger or OpenTelemetry Collector (gRPC port 4317)
    kumo::otel::init("my-crawler", "http://localhost:4317").await?;

    CrawlEngine::builder()
        .concurrency(8)
        .run(MySpider)
        .await?;

    kumo::otel::shutdown(); // flush remaining spans before exit
    Ok(())
}
```

To try it locally with Jaeger:

```bash
docker run -p 16686:16686 -p 4317:4317 jaegertracing/all-in-one
RUST_LOG=kumo=debug cargo run --features otel --example link_extractor
# open http://localhost:16686
```

### Sitemap Crawling

`SitemapSpider` reads sitemaps and emits structured `SitemapEntry` items:

```rust
// Crawl sitemap.xml — emits SitemapEntry items with loc, lastmod, priority, changefreq
CrawlEngine::builder()
    .run(SitemapSpider::new("https://example.com"))
    .await?;

// Discover sitemaps from robots.txt automatically
CrawlEngine::builder()
    .run(SitemapSpider::from_robots("https://example.com"))
    .await?;

// Only follow blog URLs
CrawlEngine::builder()
    .run(
        SitemapSpider::new("https://example.com")
            .filter_url(|url| url.contains("/blog/")),
    )
    .await?;
```

Sitemap index files are followed automatically. `from_robots()` reads `robots.txt` and extracts all `Sitemap:` directives.

### Link Extraction

`LinkExtractor` collects, filters, and deduplicates links from a response:

```rust
let links = LinkExtractor::new()
    .allow_domains(&["example.com"])    // stay on-site (subdomains included)
    .allow(r"catalogue/\d+")            // only product pages
    .deny(r"\.(pdf|zip)$")              // skip file downloads
    .restrict_css("nav.pagination")     // only links inside the pagination nav
    .canonicalize(true)                 // collapse /page#s1 and /page#s2 → /page
    .extract(&response);

Output::new().follow_many(links)
```

`allow_domains` and `allow` are OR-ed — a URL passes if either matches. `deny_domains` and `deny` are OR-ed — a URL is dropped if either matches. By default links are extracted from both `<a href>` and `<area href>`; use `.tags(&["a"])` or `.attrs(&["data-href"])` to customise.

## Feature Flags

| Flag | Pulls in | Purpose |
|---|---|---|
| _(default)_ | — | CSS + regex selectors, all stores, middleware, HTTP cache, link extractor |
| `derive` | `kumo-derive` | `#[derive(Extract)]` for zero-boilerplate CSS extraction |
| `jsonpath` | `jsonpath-rust` | JSONPath selector on `Response` |
| `xpath` | `sxd-xpath` | XPath selector on `Response` |
| `browser` | `chromiumoxide` | Headless Chromium fetcher for JS-rendered pages |
| `stealth` | `rquest`, `rquest-util` | TLS/HTTP2 fingerprint spoofing + browser stealth patches¹ |
| `claude` | `rig-core` | `AnthropicClient` for LLM extraction |
| `openai` | `rig-core` | `OpenAiClient` for LLM extraction |
| `gemini` | `rig-core` | `GeminiClient` for LLM extraction |
| `ollama` | `rig-core` | `OllamaClient` for local LLM extraction |
| `llm` | `rig-core`, `schemars` | Base LLM traits (implied by all provider flags) |
| `postgres` | `sqlx` | `PostgresStore` |
| `sqlite` | `sqlx` | `SqliteStore` |
| `mysql` | `sqlx` | `MySqlStore` |
| `persistence` | — | `FileFrontier` — file-backed URL frontier that survives restarts |
| `redis-frontier` | `redis` | `RedisFrontier` — distributed URL frontier via Redis |
| `otel` | `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `tracing-opentelemetry` | OTLP/gRPC export of all spans and events to Jaeger, Grafana Tempo, Datadog, Honeycomb, etc. |

> ¹ The `stealth` feature compiles BoringSSL from source. It requires **cmake** and **nasm** on the build machine (`apt install cmake nasm` on Ubuntu, `brew install cmake nasm` on macOS).

## License

MIT
