---
description: Tips for squeezing maximum throughput and minimum memory out of kumo in production.
---

# Performance

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

## Connection Pool

kumo automatically sets `pool_max_idle_per_host` to match the crawl's concurrency level, keeping connections warm across the full request window. You can tune the underlying `reqwest::Client` further via `.http_client_builder()`:

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
