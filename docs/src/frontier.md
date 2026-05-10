# Frontiers and Scheduling

The frontier stores pending crawl requests and deduplication state. The
scheduler decides when those requests are eligible to run.

This split matters in production crawls:

- `Frontier` stores pending requests.
- `CrawlScheduler` applies priority, fingerprint-based deduplication, retry
  timing, and politeness rules.
- `CrawlEngine` drives workers and flushes the frontier before shutdown.

The default setup works without configuration.

## MemoryFrontier (default)

The default frontier is held in RAM and is lost when the process exits.
It supports `CrawlRequest` priority scheduling: higher priority requests are
fetched first, and requests with equal priority keep FIFO order.

```rust
CrawlEngine::builder()
    // no .frontier() call: uses MemoryFrontier automatically
    .run(MySpider)
    .await?;
```

## FileFrontier

Requires `features = ["persistence"]`. Persists the request queue to disk and is
flushed by the engine before shutdown. It preserves request method, headers,
body, priority, metadata, retry count, delayed retry timing, and dedup state.

```toml
kumo = { version = "0.1", features = ["persistence"] }
```

```rust
use kumo::FileFrontier;

CrawlEngine::builder()
    .frontier(FileFrontier::open("frontier")?)
    .run(MySpider)
    .await?;
```

If the frontier directory exists when the process starts, crawling resumes from
where it left off. Delete the directory to start fresh.

## RedisFrontier

Requires `features = ["redis-frontier"]`. Distributes the request queue across
multiple processes via Redis.

```toml
kumo = { version = "0.1", features = ["redis-frontier"] }
```

```rust
use kumo::RedisFrontier;

let frontier = RedisFrontier::new(
    "redis://127.0.0.1:6379",
    "my-crawl:queue",
    "my-crawl:seen",
).await?;

CrawlEngine::builder()
    .frontier(frontier)
    .run(MySpider)
    .await?;
```

Multiple processes can use the same Redis queue and seen keys. They share the
queue and deduplication set.

## PolitenessPolicy

Use `PolitenessPolicy` to limit pressure on each domain:

```rust
use std::time::Duration;
use kumo::prelude::*;

CrawlEngine::builder()
    .concurrency(32)
    .politeness(
        PolitenessPolicy::new()
            .per_domain_concurrency(2)
            .per_domain_delay(Duration::from_millis(500)),
    )
    .run(MySpider)
    .await?;
```

`.crawl_delay(duration)` is still available as shorthand for setting the default
per-domain scheduler delay.

## FingerprintPolicy

The scheduler deduplicates requests by fingerprint. The default fingerprint
normalizes host casing, removes URL fragments, and sorts query parameters.

```rust
CrawlEngine::builder()
    .fingerprint_policy(
        FingerprintPolicy::default().strip_tracking_params(true),
    )
    .run(MySpider)
    .await?;
```

Tracking-parameter stripping removes `utm_*`, `fbclid`, and `gclid`.

`CrawlRequest::dont_filter(true)` bypasses request deduplication for an
individual request. This is useful for deliberate revisits such as retrying a
page after a state change or fetching the same endpoint with a different
request body.

## Tuning the Bloom Filter

`MemoryFrontier` uses a Bloom filter for deduplication. The default is sized for
1 million unique request fingerprints. For small crawls, reduce it to save
memory; for very large crawls, increase it to reduce false-positive skips:

```rust
CrawlEngine::builder()
    .max_urls(100_000)   // right-size for your crawl
    .run(MySpider)
    .await?;
```

Setting `max_urls` too low increases the false-positive rate, meaning some new
request fingerprints may be skipped as duplicates. Setting it too high wastes
memory. Rule of thumb: set it to twice your expected unique request count.
