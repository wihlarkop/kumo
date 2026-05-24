---
description: kumo and kumo-derive release history - features, fixes, and breaking changes by version.
---

# Changelog

Full release notes are on [GitHub Releases](https://github.com/wihlarkop/kumo/releases).

`kumo` and `kumo-derive` are versioned independently - one may release without the other.

---

## kumo

### Unreleased

- Hardened `FileFrontier` flushes by syncing temporary state files before
  replacing `queue.json` and `seen.json`, with best-effort directory sync on
  Unix platforms.
- Added `CrawlStats::retry_exhausted`, per-domain retry exhaustion counters,
  and JSON report export for exhausted retries.

### 0.3.2 - 2026-05-23

- Added stable JSON export helpers for `CrawlReport`, including compact and
  pretty-printed report strings.

### 0.3.1 - 2026-05-23

- Made `max_duration()` wake crawls promptly even when the scheduler is waiting
  on longer politeness or retry delays.

### 0.3.0 - 2026-05-23

- Expanded `FileFrontier` resume coverage for `dont_filter` requests and
  scheduler-normalized dedup fingerprints.
- Made `CachingFetcher` bypass non-GET requests and expanded HTTP cache coverage
  for TTL refetching, cached statuses, and binary-response bypass behavior.
- Added crawl budget controls: `max_pages()`, `max_items()`, `max_duration()`,
  `max_errors()`, and `CrawlStats::stop_reason`.

### 0.2.12 - 2026-05-23

- Added `CrawlStats::record_error()` so global error counts and per-domain
  failure counts can be updated together.
- Kept single-spider live metrics snapshots current after robots-blocked,
  retry, permanent failure, and task-panic events, not only successful pages.
- Documented the current `stealth` dependency risk: the optional upstream
  `rquest 5.1.0` dependency is yanked on crates.io, so `stealth` remains
  experimental until upstream publishes a healthy replacement.
- Added a CI warning annotation for the locked yanked `rquest` dependency so
  future releases keep the `stealth` risk visible without breaking unrelated
  checks.

### 0.2.11 - 2026-05-20

- Published the main `kumo` crate with the `derive` feature pointing at
  `kumo-derive 0.1.3`, so users get the latest derive diagnostics and
  `attr` + `re` extraction behavior through `kumo`.
- Hardened the publish workflow so crates.io "already exists" races are treated
  as successful publishes instead of leaving a red release run after the crate
  is already available.

### 0.2.10 - 2026-05-16

- Polished release-facing README, crate metadata, and docs text for the `0.2.x`
  line.
- Aligned getting-started requirements with the crate MSRV
  (`rust-version = "1.88"`).
- Added `production_crawler.rs`, a runnable production crawler template covering
  robots.txt, per-domain politeness, jitter, retry status filtering,
  `Retry-After`, `StatusRetry`, persistent `FileFrontier` recovery state,
  metrics, and JSONL storage.
- Updated release checklist notes for the current `0.2.x` publish flow.

### 0.2.9 - 2026-05-16

- Added `FileFrontier::state()` and `FileFrontierState` for inspecting
  recovered queue, seen, and flush configuration after opening persisted state.
- Made `FileFrontier::flush_every(0)` disable automatic flushes instead of
  risking invalid flush interval behavior; explicit and engine shutdown flushes
  still persist state.
- Expanded `FileFrontier` recovery tests and documented exact resume guarantees
  and current in-flight crash recovery limits.

### 0.2.8 - 2026-05-16

- Added `Retry-After` support for `StatusRetry` without changing
  `KumoError::HttpStatus`.
- Retry scheduling now prefers valid `Retry-After` hints and caps them with
  `RetryPolicy::max_delay`.
- Added public retry delay helper coverage for delta-seconds, HTTP-date,
  invalid header fallback, and capped delay hints.

### 0.2.7 - 2026-05-15

- Count crawl task panics as request failures in both single-spider and
  multi-spider runs.
- Attribute task-panic failures to the correct domain in `CrawlStats`.
- Strengthened stats coverage for scheduled, completed, deduped, failed, and
  panic paths.

### 0.2.6 - 2026-05-15

- Added public `RetryPolicy` introspection helpers for retry count, delay
  bounds, jitter state, status filters, and retryable error classification.
- Added `KumoError::http_status()`, `KumoError::status_code()`, and
  `KumoError::url()` helpers without changing existing error variants.
- Fixed retry middleware documentation so `StatusRetry` and `RetryPolicy`
  examples match the current API.

### 0.2.5 - 2026-05-15

- Added `KumoErrorKind` and `KumoError::kind()` for stable error
  classification without string matching.
- Added ergonomic constructors for invalid URL, LLM, and browser errors.
- Cleaned parse/store error display text and strengthened error source tests.

### 0.2.4 - 2026-05-15

- Added robots.txt `Crawl-delay` parsing for matching user-agent groups.
- Wired cached robots crawl-delay values into the scheduler when
  `PolitenessPolicy::respect_robots_crawl_delay(true)` is enabled.
- Added robots parser and scheduler coverage for crawl-delay behavior.

### 0.2.3 - 2026-05-15

- Applied `PolitenessPolicy::jitter` to per-domain scheduler delays so
  completed requests can spread follow-up traffic by a random extra delay.
- Added unit and scheduler coverage for jitter delay behavior.

### 0.2.2 - 2026-05-15

- Hardened `FileFrontier` flushes by writing temporary files before replacing
  persisted queue and seen state.
- Documented `FileFrontier` resume guarantees and the current in-flight crash
  recovery limitation.

### 0.2.1 - 2026-05-15

- Fixed scheduler fairness when delayed or domain-blocked requests share a
  frontier with ready requests from other domains.
- Reduced idle polling by letting the engine sleep until the scheduler's next
  known ready time when all queued requests are delayed.
- Added regression coverage for delayed high-priority requests.

### 0.2.0 — 2026-05-10

- Added production crawl scheduler controls with `PolitenessPolicy`.
- Added scheduler-level delayed retry timing so retry waits do not occupy
  worker tasks.
- Added `FingerprintPolicy` for canonical request deduplication.
- Added `CrawlStats`, `CrawlReport`, and per-domain scheduler counters.
- Added frontier flushing on engine shutdown so persistent frontiers can resume
  queued request state more reliably.

### 0.1.1 — 2026-05-10

- Updated crate metadata so the crates.io documentation link points to
  `https://kumo.wihlarkop.com`.

### 0.1.0 — 2026-05-10

- Fixed `CrawlEngine::stream()` cancellation so dropping the item stream stops
  the background crawl after the next attempted send instead of continuing to
  drain the frontier.
- Updated `rustls-webpki` in `Cargo.lock` to address `RUSTSEC-2026-0104`.
- Added security policy and audit configuration for the optional MySQL
  dependency advisory that has no upstream fix yet.
- Hardened release CI with broad feature checks, docs build checks, package
  dry-runs, and separate publish paths for `kumo` and `kumo-derive`.
- Added `CrawlRequest` for follow-up request scheduling with priority,
  custom headers, method/body, metadata, and per-request duplicate-filter
  bypass.
- Renamed the middleware request context to `FetchRequest` before the 0.1.0
  release to avoid confusion with spider-scheduled crawl requests.

- `CloudStore` — backend-agnostic cloud storage via [`object_store`](https://docs.rs/object_store); supports S3, GCS, Azure Blob, local filesystem, and in-memory backends through a unified `Arc<dyn ObjectStore>` interface
- New feature flags: `cloud`, `cloud-s3`, `cloud-gcs`, `cloud-azure`
- `CloudFormat::Jsonl` (default) and `CloudFormat::Json` output formats
- Auto-generated timestamped filenames; configurable via `.filename()` and `.prefix()`

#### Initial feature set

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

### 0.1.3 - 2026-05-16

- Added clear compile-time diagnostics for unsupported field types. The derive
  macro now accepts only `String` and `Option<String>` fields instead of
  producing later Rust type errors.
- Added a compile-time diagnostic for fields with multiple `#[extract(...)]`
  attributes.
- Made `attr` and `re` compose: when both are present, the macro reads the
  attribute first and then applies the regex to that attribute value.
- Updated `kumo-derive` README and crate metadata for the `kumo 0.2` line.

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

