---
description: Tips for squeezing maximum throughput and minimum memory out of kumo in production.
---

# Performance

## Measure Extraction Work

Kumo's benchmark results record root CSS queries, nested CSS queries, text
collections, and attribute reads. These operation counts verify that two
performance runs completed the same extraction work before their throughput is
compared.

The Criterion hot-path suite measures document parsing, cached root selection,
nested selection, text collection, and attribute copying independently. Use
those focused measurements to identify the next bottleneck; do not infer a
specific extraction cost from the end-to-end crawl time alone.

## Reuse Compiled Selectors

The regular `css(&str)` API uses a global compiled-selector cache, which avoids
reparsing selector syntax. Very hot loops still pay for a cache lock and hash
lookup on every call. Use `CssSelector` to compile once and bypass that lookup:

```rust
let products = CssSelector::parse("article.product_pod")?;
let titles = CssSelector::parse("h3 a")?;

for product in response.css_with(&products).iter() {
    let title = product.css_with(&titles).first().map(|element| element.text());
}
```

Keep selectors on the spider or another long-lived configuration value rather
than compiling them inside `Spider::parse`.

## Reuse Parsed Responses

Kumo parses each text `Response` into an HTML document lazily on the first CSS
query and reuses that document for later response and nested element queries.
Cloned `Element` values keep the shared document alive, so they remain usable
after the original `Response` is dropped.

Prefer several selectors against the same response or element instead of
creating new `Response` values from serialized HTML. `text()`, `attr()`, and
nested `css()` operate directly on the shared document. `outer_html()` performs
serialization only when requested and caches the result for that element.

## Request URL Metadata

Kumo parses each immutable `CrawlRequest` URL lazily and shares the resulting
URL and normalized domain metadata across cloned requests. Fingerprinting,
politeness scheduling, robots handling, allowed-domain checks, statistics, and
events reuse that metadata automatically.

No crawler configuration is required. Requests restored from a persistent
frontier rebuild their metadata lazily, so persisted state remains compatible
and does not contain derived cache fields.

## Request Task Ownership

Kumo's engine keeps one frontier-record copy for task panic recovery while the
spawned task borrows its own record during fetching and parsing. This avoids an
additional request clone for every dispatched page without weakening scheduler
cleanup after a task panic.

Lifecycle event URL and domain strings are also created lazily. Crawls that do
not configure an event receiver or hook avoid those event payload allocations
automatically; enabling events or hooks preserves the same owned event values.

Robots-blocked, retry, and permanent-failure engine paths borrow request URL and
domain metadata for stats, middleware, and tracing, then allocate owned event
payload strings only when events or hooks are enabled. Per-domain stats updates
also use a single map-entry lookup, keeping large single-domain crawls from
paying extra bookkeeping work on every counter update.

Request tasks also cache whether events or hooks are enabled before entering the
item loop. When observability is disabled, item-scraped and item-dropped hot
paths bypass event dispatch checks and avoid owned event payload allocation
entirely.

## Allocator: jemalloc

For long-running crawls (minutes or longer), replacing the system allocator with [jemalloc](https://github.com/tikv/jemallocator) can improve throughput by reducing allocator fragmentation and contention under concurrent workloads.

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

!!! note
    jemalloc pre-allocates arena space, so peak RSS will appear higher than the system allocator. This is expected — it is not a memory leak. The benefit shows up as reduced fragmentation and better multi-threaded allocation throughput over time.

## Concurrency Tuning

The right concurrency value depends on your target site's capacity:

| Scenario | Recommended |
|---|---|
| Polite crawl (public site) | 8–16 |
| Internal / scraping-allowed site | 32–64 |
| Local mock / benchmarking | 64–128 |

```rust
CrawlEngine::builder()
    .concurrency(32)
    .run(MySpider)
    .await?;
```

Use the manually dispatched `Benchmark` workflow in `scale` mode to measure
Kumo at concurrency `1`, `4`, `8`, `16`, `32`, and `64`. Scale mode uses 64
independent pagination chains so the frontier contains enough runnable work to
exercise concurrency. It runs each level three times and reports median
throughput, elapsed time, peak RSS, fetch/parse time, and peak requests in
flight.

## Validate Large Crawls

Use the manual benchmark workflow before making production-scale claims:

| Mode | Pages | Items |
|---|---:|---:|
| `soak` | 500 | 10,000 |
| `large` | 5,000 | 100,000 |

These workloads use 100 independent pagination chains and validate the engine,
frontier, extraction, and JSONL store together. A run fails on an incorrect
page count, incorrect item count, duplicate item, malformed JSONL output, or
unsuccessful crawler process.

The report includes peak RSS and RSS per 1,000 items. It also compares
throughput for the first 10,000 items with the remaining crawl. Do not treat one
shared-runner result as a stable memory limit. Establish three consecutive
successful 100k runs before publishing a large-crawl claim, then use those runs
to define a bounded RSS envelope.

## Large Pending Queues

`MemoryFrontier` uses a binary heap for request priority scheduling. Push and
pop operations are `O(log n)`, including when many requests are waiting at
once. Equal-priority requests still preserve FIFO order.

Use the manual `Benchmark` workflow's `frontier` mode to measure isolated
100-request and 10,000-request push/drain batches. This benchmark bypasses
deduplication so Bloom-filter false positives cannot affect the measured queue
size.

Use `scheduler` mode to measure complete push, dispatch, and finish lifecycles
at the same queue sizes.

## Validate Retry Resilience

The manual `Benchmark` workflow's `realistic` mode exercises Kumo against a
deterministic server with variable 20-120 ms latency, 1-128 KiB response
payloads, and first-attempt HTTP 429/503 failures.

The workload contains 200 pages and 4,000 unique items over 20 independent
pagination chains. Kumo must recover exactly 24 transient failures with no
exhausted retries or final crawl errors. The report cross-checks crawler output
with server-side request and status counters, so a green result proves both
correct retry behavior and complete extraction.

Use this mode for production-behavior regressions. Use the nginx local mode for
raw framework-overhead comparisons. Shared GitHub runner timing remains
informational; correctness and retry counters are deterministic release gates.

Use `realistic-compare` when comparing Kumo with Scrapy and Colly. The server
state is reset before every framework. The default three-run schedule rotates a
seeded framework permutation so every framework runs once in each position.
The report is rejected unless every framework in every run independently
satisfies the same item, page, duplicate, retry, error, and HTTP status
counters. Public comparisons should use the reported medians and ranges rather
than one shared-runner sample.

## Connection Pool

kumo automatically sets `pool_max_idle_per_host` to match the crawl's
concurrency level, keeping connections warm across the full request window.
Each URL selected by `ProxyRotator` receives its own cached client, cookie jar,
and connection pool. Proxy clients inherit Kumo's concurrency, request timeout,
User-Agent, and TCP keepalive settings.

You can tune the default `reqwest::Client` further via
`.http_client_builder()`:

```rust
CrawlEngine::builder()
    .concurrency(32)
    .http_client_builder(|b| {
        b.pool_max_idle_per_host(32)
         .tcp_keepalive(std::time::Duration::from_secs(60))
    })
    .run(MySpider)
    .await?;
```

The callback is applied once to the default reqwest client. It is not replayed
for dynamically created proxy clients and does not configure the wreq-backed
stealth client.

## Request Timeout

Hanging connections can stall the crawl engine. Set a per-request timeout to bound worst-case latency:

```rust
CrawlEngine::builder()
    .request_timeout(std::time::Duration::from_secs(30))
    .run(MySpider)
    .await?;
```

## TLS and HTTP/2

kumo uses rustls (pure-Rust TLS) and HTTP/2 by default. No additional configuration is needed — sites that support HTTP/2 will automatically benefit from request multiplexing over fewer connections.

## Disable robots.txt for Internal Crawls

By default kumo fetches `robots.txt` for every new domain — one extra HTTP round-trip per domain. For internal or authorized targets where you control the server, disable it:

```rust
CrawlEngine::builder()
    .respect_robots_txt(false)
    .run(MySpider)
    .await?;
```

## Bloom Filter Sizing

kumo uses a Bloom filter for URL deduplication. The default is sized for 1 million unique URLs. For small crawls, reduce it to save memory; for very large crawls, increase it to reduce false-positive skips:

```rust
// Small crawl — save ~1 MB of memory
CrawlEngine::builder()
    .max_urls(10_000)
    .run(MySpider)
    .await?;

// Large crawl — 10M URLs with low false-positive rate
CrawlEngine::builder()
    .max_urls(10_000_000)
    .run(MySpider)
    .await?;
```

## Store Choice

`JsonlStore` is the fastest store — it is append-only and never blocks on index lookups or transactions. For maximum throughput, write to JSONL and bulk-load into a database afterwards:

```rust
// Fast — append-only writes
CrawlEngine::builder()
    .store(JsonlStore::new("items.jsonl")?)
    .run(MySpider)
    .await?;
```

If you need a database store, prefer `SqliteStore` for single-process crawls and `PostgresStore` for distributed ones. Avoid using a database store as the primary bottleneck in a high-concurrency crawl.

## Don't Stack AutoThrottle and RateLimiter

`AutoThrottle` and `RateLimiter` both add delays — using both at the same time compounds them independently and will significantly reduce throughput. Pick one:

- Use `RateLimiter` when you want a fixed maximum request rate.
- Use `AutoThrottle` when you want the engine to adapt automatically based on server response times.

```rust
// ✅ Pick one
CrawlEngine::builder()
    .middleware(AutoThrottle::new())  // OR RateLimiter, not both
    .run(MySpider)
    .await?;
```

## Stream Buffer Tuning

When using `CrawlEngine::stream()`, the default channel buffer is 100 items. If your consumer is slow (e.g. writing to a database row-by-row), the buffer fills up and backpressure stalls the crawl. Increase it to decouple producer and consumer:

```rust
let stream = CrawlEngine::builder()
    .stream_buffer(1_000)
    .stream(MySpider)
    .await?;
```

## Scheduler Politeness

`PolitenessPolicy` enforces per-domain concurrency and delay before requests are
dispatched. Use it instead of sleeping inside spiders or middleware:

```rust
CrawlEngine::builder()
    .concurrency(64)
    .politeness(
        PolitenessPolicy::new()
            .per_domain_concurrency(4)
            .per_domain_delay(std::time::Duration::from_millis(250)),
    )
    .run(MySpider)
    .await?;
```

High global concurrency is useful only when the crawl spans enough domains or
the target allows that load. For single-domain public crawls, the per-domain
limits are usually the real throughput cap.

## HTTP Cache for Development

Use `.http_cache()` during spider development to avoid re-fetching pages on every run. Cached responses are served from disk instantly, making iteration fast. Remove it before deploying to production:

```rust
CrawlEngine::builder()
    .http_cache("./dev-cache")?
    .cache_ttl(std::time::Duration::from_secs(3600)) // optional: expire after 1h
    .run(MySpider)
    .await?;
```

## Depth and Domain Filtering

Without limits, a spider following `<a>` tags can crawl the entire internet. Always set `allowed_domains()` and consider `max_depth()` on your spider to keep crawls focused:

```rust
impl Spider for MySpider {
    fn allowed_domains(&self) -> Vec<&str> {
        vec!["example.com"]
    }

    fn max_depth(&self) -> Option<usize> {
        Some(3) // follow links up to 3 levels deep
    }
    // ...
}
```

