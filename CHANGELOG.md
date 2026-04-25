# Changelog

All notable changes to kumo and kumo-derive are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [0.1.0] — 2026-04-13

### kumo — Added

- `CrawlEngine::builder()` — fluent builder for configuring and launching crawls
- `Spider` trait — type-safe spider with associated `Item` type
- CSS, regex, XPath (`xpath` feature), JSONPath (`jsonpath` feature) selectors
- `#[derive(Extract)]` — zero-boilerplate CSS extraction from field annotations (`derive` feature)
- LLM extraction via Claude, OpenAI, Gemini, Ollama (`claude`/`openai`/`gemini`/`ollama` features)
- LLM fallback — `#[extract(llm_fallback = "hint")]` tries CSS first, falls back to LLM
- `RateLimiter`, `AutoThrottle`, `StatusRetry`, `ProxyRotator`, `UserAgentRotator` middleware
- `RetryPolicy` — exponential backoff with jitter, per-status filtering
- `JsonlStore`, `JsonStore`, `CsvStore`, `StdoutStore`
- `PostgresStore`, `SqliteStore`, `MySqlStore` (`postgres`/`sqlite`/`mysql` features)
- `DropDuplicates`, `FilterPipeline`, `RequireFields` item pipelines
- `MemoryFrontier`, `FileFrontier` (`persistence` feature), `RedisFrontier` (`redis-frontier` feature)
- Bloom filter URL deduplication (O(1), configurable via `max_urls`)
- `LinkExtractor` — link collection with allow/deny regex, `restrict_css`, canonicalization
- Disk-backed HTTP response cache with optional TTL
- robots.txt per-domain fetch and cache
- Headless browser fetcher via chromiumoxide (`browser` feature)
- Stealth mode — TLS/HTTP2 fingerprint spoofing via rquest (`stealth` feature)
- `BrowserConfig::stealth()` — JavaScript API patching for bot-detection evasion
- Multi-spider engine — `add_spider()` / `run_all()`
- `CrawlEngine::stream()` — async `Stream` of items with configurable backpressure buffer
- `SitemapSpider` — reads sitemap.xml and sitemap index files, emits `SitemapEntry` items
- OpenTelemetry OTLP/gRPC export of all spans and events (`otel` feature)
- `MockFetcher` and `CachingFetcher` for testing
- `metrics_interval` — periodic stats logging via `tracing`

### kumo-derive — Added

- `#[derive(Extract)]` proc-macro for structs with named fields
- `css = "selector"` — required CSS selector per field
- `attr = "name"` — extract HTML attribute instead of text
- `re = r"pattern"` — apply regex, take first match
- `text` — explicit text extraction (default)
- `default = "value"` — fallback for `String` fields
- `transform = "trim|lowercase|uppercase"` — post-extraction transform
- `llm_fallback = "hint"` / `llm_fallback` — fall back to LLM when selector is empty
- `String` fields fall back to `""`, `Option<String>` fields to `None`

[Unreleased]: https://github.com/wihlarkop/kumo/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/wihlarkop/kumo/releases/tag/v0.1.0
