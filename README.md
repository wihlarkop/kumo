# kumo

[![CI](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml/badge.svg)](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-kumo.wihlarkop.com-blue)](https://kumo.wihlarkop.com)

<p align="center">
  <img src="assets/logo.png" alt="kumo logo" width="200">
</p>

An async web crawling framework for Rust — Scrapy for Rust.

**kumo** (蜘蛛/雲 — spider/cloud) gives you a trait-based, async-first API for writing spiders that scrape, follow links, and store data.

## Why Kumo?

|  | **kumo** | **Scrapy** (Python) | **Colly** (Go) |
|---|---|---|---|
| Language | Rust | Python | Go |
| Type safety | Compile-time | Runtime | Partial |
| Async model | Tokio (true async) | Twisted (event loop) | goroutines |
| Memory safety | Guaranteed | GC | GC |
| CSS / XPath / Regex / JSONPath | ✓ | ✓ | CSS only |
| `#[derive(Extract)]` macro | ✓ | ✗ | ✗ |
| LLM extraction (Claude / OpenAI / Gemini / Ollama) | ✓ | ✗ | ✗ |
| Browser / JS rendering | ✓ (chromiumoxide) | ✓ (Playwright) | ✗ |
| Stealth mode (TLS/HTTP2 fingerprint spoofing) | ✓ | ✗ | ✗ |
| Distributed frontier (Redis) | ✓ | ✓ (scrapy-redis) | ✗ |
| Item stream (Kafka, WebSocket) | ✓ | ✗ | ✗ |
| OpenTelemetry export | ✓ | ✗ | ✗ |
| Pluggable stores (JSONL, CSV, Postgres, SQLite, MySQL) | ✓ | ✓ (pipelines) | ✗ |
| Single binary deploy | ✓ | ✗ | ✓ |
| Binary size / startup | Small / instant | Large / slow | Small / fast |

**Benchmark results** — 1 000 books, concurrency 16, median of 3 runs:

| | **kumo** | Colly (Go) | Scrapy (Python) |
|---|---|---|---|
| Real site — Items/s | **68.4** | 68.8 | 51.4 |
| Local server — Items/s | **12 346** | 4 310 | 179 |
| Peak RSS | **14.2 MB** | 33.6 MB | 77.1 MB |

On raw parsing throughput (local server, no network): **2.9× faster than Colly, 69× faster than Scrapy**. See [`benchmark/`](benchmark/) for full methodology and reproduction steps.

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
- **Item stream** — `CrawlEngine::stream()` returns an async `Stream` for real-time item consumption
- **robots.txt** — per-domain fetch + cache, enabled by default
- **Bloom filter dedup** — O(1) URL deduplication, 1M URLs at 0.1% false-positive rate
- **HTTP cache** — disk-backed response cache via `.http_cache(dir)`, optional TTL
- **Link extractor** — `LinkExtractor` with allow/deny regex, `allow_domains`, `canonicalize`, `restrict_css`
- **Pluggable storage** — `JsonlStore`, `JsonStore`, `CsvStore`, `StdoutStore`, PostgreSQL, SQLite, MySQL
- **Middleware chain** — proxy rotation, custom headers, rate limiting, auto-throttle
- **Domain + depth filtering** — `allowed_domains()` and `max_depth()` on the `Spider` trait
- **Multi-spider engine** — run multiple spiders concurrently via `.add_spider()` / `.run_all()`
- **LLM extraction** — extract structured data without selectors using Claude, OpenAI, Gemini, or Ollama
- **Browser fetcher** — headless Chromium via chromiumoxide for JS-rendered pages (feature: `browser`)
- **Distributed frontier** — Redis-backed URL frontier for multi-process crawls (feature: `redis-frontier`)
- **Persistent frontier** — file-backed URL frontier that survives restarts (feature: `persistence`)
- **Sitemap spider** — `SitemapSpider` reads `sitemap.xml` and sitemap index files
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

For `#[derive(Extract)]`:

```toml
kumo = { version = "0.1", features = ["derive"] }
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

## Performance Tips

For long-running crawls (minutes or longer), using [jemalloc](https://github.com/tikv/jemallocator) as your global allocator can improve throughput by reducing allocator fragmentation and contention under concurrent workloads:

```toml
# Cargo.toml
[dependencies]
tikv-jemallocator = "0.6"
```

```rust
// main.rs
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
```

Note: jemalloc pre-allocates arena space so peak RSS will appear higher than the system allocator — this is expected and not a memory leak.

## Documentation

Full documentation at **[kumo.wihlarkop.com](https://kumo.wihlarkop.com)**

- [Getting Started](https://kumo.wihlarkop.com/getting-started/)
- [Spiders](https://kumo.wihlarkop.com/spiders/)
- [Extractors](https://kumo.wihlarkop.com/extractors/)
- [derive(Extract)](https://kumo.wihlarkop.com/derive/)
- [Middleware](https://kumo.wihlarkop.com/middleware/)
- [Stores](https://kumo.wihlarkop.com/stores/)
- [Advanced topics](https://kumo.wihlarkop.com/advanced/stream/) — item stream, OpenTelemetry, stealth, browser, and more
- [Examples](https://kumo.wihlarkop.com/examples/)
- [Feature Flags](https://kumo.wihlarkop.com/feature-flags/)

## License

MIT
