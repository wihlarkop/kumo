---
description: kumo and kumo-derive release history — features, fixes, and breaking changes by version.
---

# Changelog

Full release notes are on [GitHub Releases](https://github.com/wihlarkop/kumo/releases).

`kumo` and `kumo-derive` are versioned independently — one may release without the other.

---

## kumo

### Unreleased

- `CloudStore` — backend-agnostic cloud storage via [`object_store`](https://docs.rs/object_store); supports S3, GCS, Azure Blob, local filesystem, and in-memory backends through a unified `Arc<dyn ObjectStore>` interface
- New feature flags: `cloud`, `cloud-s3`, `cloud-gcs`, `cloud-azure`
- `CloudFormat::Jsonl` (default) and `CloudFormat::Json` output formats
- Auto-generated timestamped filenames; configurable via `.filename()` and `.prefix()`

### 0.1.0 — 2026-04-13

- Async-first crawl engine via Tokio (`CrawlEngine::builder()`)
- CSS, regex, XPath, JSONPath selectors
- LLM extraction via Claude, OpenAI, Gemini, Ollama
- Rate limiting, auto-throttle, retry with backoff
- `JsonlStore`, `JsonStore`, `CsvStore`, `StdoutStore`
- PostgreSQL, SQLite, MySQL stores
- Item pipelines (`DropDuplicates`, `FilterPipeline`, `RequireFields`)
- `MemoryFrontier`, `FileFrontier`, `RedisFrontier`
- `LinkExtractor` with allow/deny filtering
- HTTP response cache, Bloom filter dedup, robots.txt
- Headless browser fetcher, stealth mode
- Multi-spider engine
- `CrawlEngine::stream()` — async item stream with backpressure
- `SitemapSpider`
- OpenTelemetry OTLP/gRPC export (`otel` feature)

---

## kumo-derive

### 0.1.2 — 2026-04-25

- Added `default = "value"` — fallback string for `String` fields
- Added `transform = "trim|lowercase|uppercase"` — post-extraction transform

### 0.1.1 — 2026-04-25

- Added crate metadata: `authors`, `rust-version`, `documentation`, `exclude`

### 0.1.0 — 2026-04-21

- `#[derive(Extract)]` proc-macro for structs with named fields
- `css`, `attr`, `re`, `text` field options
- `llm_fallback` — CSS-first with LLM fallback
- `String` fields default to `""`, `Option<String>` to `None`
