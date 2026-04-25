---
description: kumo and kumo-derive release history — features, fixes, and breaking changes by version.
---

# Changelog

Full release notes are on [GitHub Releases](https://github.com/wihlarkop/kumo/releases).

## kumo 0.1.0 · kumo-derive 0.1.0

Initial release.

### kumo

- Async-first crawl engine via Tokio (`CrawlEngine::builder()`)
- CSS, regex, XPath, JSONPath selectors
- `#[derive(Extract)]` for zero-boilerplate CSS extraction
- LLM extraction via Claude, OpenAI, Gemini, Ollama
- Rate limiting, auto-throttle, retry with backoff
- `JsonlStore`, `JsonStore`, `CsvStore`, `StdoutStore`
- PostgreSQL, SQLite, MySQL stores
- Item pipelines (`DropDuplicates`, `FilterPipeline`, `RequireFields`)
- `MemoryFrontier`, `FileFrontier`, `RedisFrontier`
- `LinkExtractor` with allow/deny filtering
- HTTP response cache
- Bloom filter URL deduplication
- robots.txt support
- Headless browser fetcher via chromiumoxide
- Stealth mode — TLS/HTTP2 fingerprint spoofing
- Multi-spider engine
- `CrawlEngine::stream()` — async item stream with backpressure
- `SitemapSpider`
- OpenTelemetry OTLP/gRPC export (`otel` feature)

### kumo-derive

- `#[derive(Extract)]` proc-macro for structs with named fields
- `css = "selector"` — required CSS selector per field
- `attr = "name"` — extract HTML attribute instead of text
- `re = r"pattern"` — apply regex, take first match
- `text` — explicit text extraction (default)
- `default = "value"` — fallback for `String` fields
- `transform = "trim|lowercase|uppercase"` — post-extraction transform
- `llm_fallback = "hint"` / `llm_fallback` — fall back to LLM when selector is empty
- `String` fields fall back to `""`, `Option<String>` fields to `None`
