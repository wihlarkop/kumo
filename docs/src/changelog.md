---
description: kumo and kumo-derive release history - features, fixes, and breaking changes by version.
---

# Changelog

Full release notes are on [GitHub Releases](https://github.com/wihlarkop/kumo/releases).

`kumo` and `kumo-derive` are versioned independently - one may release without the other.

---

## kumo

### 0.5.2 - 2026-06-19

- Added a backward-compatible frontier lease/dead-letter API foundation with
  `FrontierLease`, `FrontierLeaseId`, `DeadLetterReason`, and default
  `Frontier` lifecycle methods for future durable in-flight request recovery.
- Added durable `FileFrontier` lease storage with `leases.json`, restart
  recovery back to the pending queue, lease ack/release handling, lease expiry
  reclaim, and `dead_letters.json` persistence.
- Wired the scheduler and crawl engine to use durable frontier leases when a
  frontier opts in, including acking terminal requests and dead-lettering leased
  task panics.
- Added Redis-backed delayed requests, in-progress leases, lease expiry reclaim,
  and dead-letter handling for distributed `RedisFrontier` crawls.
- Added priority-aware Redis ready queues while preserving FIFO order for equal
  priority requests and migrating older list-backed pending queues on poll.
- Added opt-in stats checkpoints that periodically write `CrawlReport` JSON and
  refresh the file with the final crawl report at shutdown.
- Durable frontiers now dead-letter leased requests when retry capacity is
  exhausted instead of only acking the terminal failure.
- Added proxy health tracking to `ProxyRotator`, including per-proxy success
  and failure counters, configurable cooldown after consecutive failed request
  attempts, shared health snapshots from cloned rotators, and a no-cooldown mode
  that preserves the existing rotation API while retaining counters.

### 0.5.1 - 2026-06-04

- Added `otel` production metrics for final crawl report counters, per-request
  successful fetch latency histograms, and bounded store queue/write signals.
- Added opt-in HTTP-first browser fallback with `browser_fallback(...)`,
  `browser_fallback_on(...)`, JavaScript-gated response detection, fallback
  failure recovery, and report JSON counters for fallback attempts, successes,
  and failures.
- Applied Kumo's connection-pool size, request timeout, User-Agent, cookie, and
  TCP keepalive policy consistently to lazily created per-proxy HTTP clients
  while preserving isolated proxy cookie jars and pools.
- Applied equivalent pool, timeout, and keepalive settings to default and
  per-proxy stealth HTTP clients.
- Reduced scheduler bookkeeping allocations by borrowing request URL/domain
  metadata in robots-blocked, retry, and permanent-failure paths unless owned
  lifecycle event payloads are actually emitted.
- Removed an extra per-domain stats map lookup from every scheduled, completed,
  retry, error, and robots-blocked counter update.
- Cached request-task observer enablement so item-scraped and item-dropped hot
  paths bypass event dispatch checks when events and hooks are disabled.
- Reduced feature-matrix CI disk pressure by freeing unused hosted-tool caches
  before the heavy LLM lane and avoiding `target` cache uploads for feature
  permutations.
- Updated GitHub Pages deployment actions to their Node 24-compatible major
  versions.
- Added `RetryReport` and `CrawlReport::retry_summary()` so production reports
  expose retry pressure, retry exhaustion, and exhausted-failure rates in one
  typed summary and JSON object.
- Added opt-in bounded store buffering with `CrawlEngine::store_buffer(...)`,
  `StoreBufferConfig`, `StoreStats`, report JSON fields, and batched
  `ItemStore::store_many()` writes for built-in file/stdout stores.
- Added explicit store-buffer outage handling with `StoreFailurePolicy::Abort`,
  first-failure reporting, failed write counters, and queue/write average and
  max latency fields in store stats and report JSON.
- Hardened the stealth CI job with explicit timeouts and retry-safe package
  installation so runner package-manager hangs cannot block PRs indefinitely.
- Added transactional `store_many()` implementations for SQLite, Postgres, and
  MySQL stores so buffered database writes commit one flushed batch per
  transaction.
- Added `CrawlTimingStats` and `CrawlReport::timings` for cumulative
  successful-request phase timings across request middleware, fetch, response
  middleware, parse, pipeline, and store work.
- Added timing breakdown fields to production report JSON exports and Kumo
  benchmark output.
- Documented how to interpret crawl timing totals under concurrent workloads.
- Reworked CSS extraction to parse each text response lazily once and share the
  parsed document across response selectors, nested element selectors, cloned
  elements, text extraction, and attribute access.
- Made `Element::outer_html()` serialize lazily while preserving the existing
  owned, cloneable element API.
- Added reusable `CssSelector` values plus `Response::css_with(...)` and
  `Element::css_with(...)` for hot loops that need to bypass repeated global
  selector-cache lookups while preserving the existing `css(&str)` API.
- Reworked the scaling benchmark to use independent crawl chains, three-run
  medians, peak in-flight request measurement, and automatic throughput/memory
  saturation reporting across concurrency 1 through 64.
- Added manual 10,000-item soak and 100,000-item large-crawl benchmark modes
  with exact page/item validation, duplicate detection, peak RSS reporting,
  memory per 1,000 items, and first-10k throughput comparison.
- Added a deterministic realistic resilience benchmark with variable latency,
  variable response sizes, recoverable HTTP 429/503 responses, and typed
  cross-validation of crawl output, retry stats, and server counters.
- Added a correctness-gated realistic comparison for Kumo, Scrapy, and Colly,
  with reset failure schedules, equivalent retry limits, multi-start crawling,
  and asynchronous Colly execution.
- Hardened the realistic framework comparison with seeded, position-balanced
  repeated runs, per-sample correctness gates, and median throughput and memory
  reports with min-max ranges.
- Added a correctness-gated, balanced CI benchmark that isolates JSONL output
  overhead from fetching, parsing, scheduling, and other crawl work.
- Reused lazy request URL and normalized domain metadata across fingerprinting,
  scheduling, robots handling, allowed-domain filtering, statistics, and
  lifecycle events.
- Removed a redundant frontier-request clone from every dispatched task and
  deferred event-only request string allocations when events and hooks are
  disabled.
- Replaced `MemoryFrontier`'s linear priority scan with a stable binary heap,
  preserving priority and FIFO behavior while making queue push/pop operations
  `O(log n)`.
- Added a CI-only frontier benchmark for 100-request and 10,000-request
  push/drain workloads.
- Added a CI-only scheduler benchmark for complete 100-request and
  10,000-request push, dispatch, and finish lifecycles.

### 0.5.0 - 2026-06-03

- Added async crawl lifecycle hooks through the `CrawlHook` trait.
- Added `HookErrorPolicy` for choosing whether hook failures are logged or
  abort the crawl.
- Added `CrawlEngine::hook(...)`, `CrawlEngine::hooks(...)`, and
  `CrawlEngine::hook_error_policy(...)`.
- Added `KumoErrorKind::Hook` and `KumoError::hook(...)` for hook failures.
- Added crawl hooks documentation, tests, and a runnable `crawl_hooks` example.

### 0.4.1 - 2026-06-03

- Added `CrawlEvent::name()` with stable snake_case event labels for
  dashboards, metrics, and event logs.
- Added crawl event coverage for stream cancellation and task panic paths.
- Documented event labels and ordering expectations for event consumers.

### 0.4.0 - 2026-06-03

- Added typed crawl lifecycle events through `CrawlEvent`.
- Added `CrawlEngine::events(...)` for caller-owned broadcast channels.
- Added `CrawlEngine::event_channel(...)` for convenient event subscription.
- Added event coverage for crawl start/finish, scheduling, skips, request
  start/completion, retries, failures, task panics, scraped items, and dropped
  items.
- Added crawl events docs and a runnable `crawl_events` example.

### 0.3.16 - 2026-06-03

- Automated GitHub Release creation in the tag-driven publish workflow after
  crates.io publish steps complete.
- Added maintainer release process notes covering version bumps, changelog
  updates, tag formats, publish verification, and recovery rules.

### 0.3.15 - 2026-06-02

- Added a stealth contract test for `StealthProfile` to `wreq-util`
  emulation mapping.
- Added stealth migration notes documenting the `rquest` to `wreq` backend
  change while keeping Kumo's public stealth API stable.

### 0.3.14 - 2026-06-01

- Replaced the optional `rquest` stealth HTTP dependency with `wreq`, keeping
  Kumo's public `StealthHttpFetcher` and `StealthProfile` API stable while
  removing the yanked upstream `rquest` dependency from the planned stealth path.
- Documented the `wreq-util` license metadata risk for stealth releases.

### 0.3.13 - 2026-06-01

- Added dedicated CI coverage for compiling default and feature-gated examples.
- Synced the repository examples README with the live examples documentation and
  documented the `polite_crawling` example in the docs site.

### 0.3.12 - 2026-06-01

- Added integration coverage for the structured logging contract so core crawl,
  request, and item-drop events keep stable `event` fields and production
  context.

### 0.3.11 - 2026-06-01

- Standardized structured tracing events with stable `event` fields across
  crawl, request, item, cache, and stream logs.
- Added more consistent production log context such as spider name, domain,
  depth, attempt, retry limits, error kind, and multi-spider index.

### 0.3.10 - 2026-06-01

- Refreshed direct dependencies including Rig, Reqwest, Tokio, scraper, Redis,
  OpenTelemetry, object_store, SQLx, and Criterion.
- Raised the minimum supported Rust version to 1.94 for SQLx 0.9.

### 0.3.9 - 2026-06-01

- Added a typed crawl events design document covering goals, candidate event
  variants, subscription API shape, and the relationship between events,
  logging, OpenTelemetry, and crawl reports.

### 0.3.8 - 2026-06-01

- Hardened duration-budget integration tests so loaded CI runners do not expire
  the crawl before the first mock response can complete.

### 0.3.7 - 2026-06-01

- Added production report docs covering `CrawlReport` health signals, alert
  examples, domain breakdowns, and operational report export patterns.

### 0.3.6 - 2026-05-31

- Added `CrawlReport` helper methods and JSON fields for pages/sec, items/sec,
  bytes/sec, error rate, success rate, and retry exhaustion rate.

### 0.3.5 - 2026-05-24

- Hardened logging with stable `tracing` event targets and event names for
  crawl lifecycle, request retry/skip, item drops, and HTTP cache activity.
- Added production logging docs covering `RUST_LOG`, structured targets, and
  JSON log setup.

### 0.3.4 - 2026-05-24

- Added `CrawlStats::error_kinds`, per-domain error-kind counters, stable
  `KumoErrorKind::as_str()` labels, and JSON report export for failure
  breakdowns.

### 0.3.3 - 2026-05-24

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

