# Changelog

All notable changes to `kumo` will be documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — 2026-04-25

### Added

**Core**
- `CrawlEngine` fluent builder — concurrency, middleware, pipelines, store, frontier, retry, robots.txt
- `Spider` trait with `parse()`, `on_error()`, `open()`, `close()`, `allowed_domains()`, `max_depth()`
- `CrawlStats` returned after each crawl (pages, items, errors, bytes, duration)
- Multi-spider support via `add_spider()` + `run_all()`
- Graceful Ctrl+C shutdown

**Fetchers**
- `HttpFetcher` — async reqwest-based HTTP fetcher
- `BrowserFetcher` — chromiumoxide headless browser fetcher (feature: `browser`)
- `StealthHttpFetcher` — TLS/HTTP2 fingerprint spoofing via rquest/BoringSSL (feature: `stealth`)
- `CachingFetcher` — disk-based HTTP response cache with optional TTL
- `MockFetcher` — test-only fetcher for running spiders without network access

**Extraction**
- `Response` with CSS, XPath (feature: `xpath`), JSONPath (feature: `jsonpath`), regex selectors
- `LinkExtractor` for automatic link following
- `#[derive(Extract)]` proc-macro (feature: `derive`) via `kumo-derive`
- LLM adaptive extraction via `llm_fallback` attribute (feature: `llm`)

**Middleware**
- `DefaultHeaders` — inject static request headers
- `UserAgentRotator` — rotate user-agent strings per request
- `RateLimiter` — token-bucket rate limiting via `governor`
- `AutoThrottle` — EWMA-based adaptive throttling
- `ProxyRotator` — HTTP/SOCKS5 proxy pool with automatic rotation
- `StatusRetry` — convert configurable HTTP status codes into retriable errors

**Retry**
- `RetryPolicy` — exponential backoff with optional jitter, max delay cap, and per-status filtering
- `.retry_policy()` builder method for full control; `.retry()` convenience wrapper

**Pipelines**
- `FilterPipeline` — drop items by predicate
- `RequireFields` — drop JSON items missing required keys
- `DropDuplicates` — deduplicate items by field value

**Stores**
- `StdoutStore`, `JsonStore`, `JsonlStore`, `CsvStore`
- `SqliteStore` (feature: `sqlite`), `PostgresStore` (feature: `postgres`), `MySqlStore` (feature: `mysql`)

**Frontiers**
- `MemoryFrontier` — default in-memory frontier with Bloom filter deduplication
- `FileFrontier` — crash-safe persistent frontier (feature: `persistence`)
- `RedisFrontier` — distributed frontier for horizontal scaling (feature: `redis-frontier`)

**LLM extraction**
- `LlmClient` trait + `AnthropicClient`, `OpenAiClient`, `GeminiClient`, `OllamaClient`
- `ResponseExtractExt` for schema-driven structured extraction from responses

**Other**
- `SitemapSpider` trait for sitemap.xml / sitemap_index.xml crawling
- `RobotsCache` with configurable TTL
- `tracing` integration throughout
- Live metrics via `.metrics_interval()`
