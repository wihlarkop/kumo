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
fetched first, and requests with equal priority keep FIFO order. Its heap-backed
queue keeps insertion and removal at `O(log n)`, so large pending queues do not
degrade into repeated linear scans.

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
kumo = { version = "0.2", features = ["persistence"] }
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

`FileFrontier` writes queued and seen state through temporary files, syncs those
temporary files, and then replaces `queue.json` and `seen.json` on flush. On
Unix platforms it also best-effort syncs the frontier directory after each
replace. Stale `*.tmp` files from an interrupted flush are ignored on resume.

It is designed for normal resume after graceful shutdown. A request that has
already been popped into an in-flight task is not checkpointed as pending; if
the process crashes after dispatch and before completion, that request may need
to be rescheduled by the application or a future lease-based frontier.

Use `FileFrontier::state().await` after opening a frontier when you want to
verify what was recovered before resuming:

```rust
let frontier = FileFrontier::open("frontier")?;
let state = frontier.state().await;
println!(
    "recovered {} queued requests and {} seen fingerprints",
    state.queued, state.seen
);
```

The persisted files contain:

| File | Contents | Survives Resume |
|------|----------|-----------------|
| `queue.json` | Pending `CrawlRequest`s with method, headers, body, priority, metadata, depth, retry count, and scheduled retry time | Yes |
| `seen.json` | Exact deduplication fingerprints used to rebuild the Bloom filter | Yes |

Automatic flushing happens every 100 pushes by default. Use
`.flush_every(n)` to tune that interval. Use `.flush_every(0)` to disable
automatic flushing and rely only on explicit `frontier.flush().await?` calls or
the engine's final shutdown flush.

## RedisFrontier

Requires `features = ["redis-frontier"]`. Distributes the request queue across
multiple processes via Redis.

```toml
kumo = { version = "0.2", features = ["redis-frontier"] }
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

Add `.jitter(duration)` when many requests may become eligible at the same time.
Kumo adds a random extra delay from zero up to the configured jitter after each
completed request for that domain.

When robots.txt contains `Crawl-delay`, Kumo uses the larger of the configured
per-domain delay and the robots delay. Disable this with
`.respect_robots_crawl_delay(false)` if your application handles robots timing
outside Kumo.

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

## Lease API Foundation

`Frontier` includes a lease/dead-letter API for durable frontier
implementations:

- `lease_request(ttl)` leases the next request for in-flight processing.
- `ack_lease(id)` marks a leased request as complete.
- `release_lease(id)` returns a leased request to the frontier.
- `dead_letter(id, reason)` records a terminal request for audit or replay.

The default implementation is compatibility-first: it wraps `pop_request()` in
an ephemeral `FrontierLease` and treats ack, release, and dead-letter calls as
no-ops. That means existing custom frontiers keep compiling and keep their
current pop-only behavior until they explicitly override the lease methods.

Durable lease persistence for `FileFrontier` and atomic Redis leases are planned
as separate slices because they change crash-recovery guarantees. Until those
implementations land, `FileFrontier` keeps its current graceful-shutdown resume
behavior described above.

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

