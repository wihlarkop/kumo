---
description: Configure kumo crawlers for production runs with bounded scope, polite scheduling, durable output, reports, and recovery state.
---

# Production Crawling

Production crawlers need explicit scope, bounded concurrency, durable output,
and enough reporting to explain what happened after the process exits. Start
with the smallest configuration that protects the target site and gives you a
repeatable audit trail.

## Baseline Configuration

`FileFrontier` requires `features = ["persistence"]`.

```rust
use std::time::Duration;

use kumo::FileFrontier;
use kumo::prelude::*;

let frontier = FileFrontier::open("production-frontier")?.flush_every(25);
let recovered = frontier.state().await;
tracing::info!(
    queued = recovered.queued,
    seen = recovered.seen,
    "frontier recovered state"
);

let result = CrawlEngine::builder()
    .concurrency(16)
    .respect_robots_txt(true)
    .politeness(
        PolitenessPolicy::new()
            .per_domain_concurrency(2)
            .per_domain_delay(Duration::from_millis(500))
            .jitter(Duration::from_millis(250)),
    )
    .retry_policy(
        RetryPolicy::new(3)
            .base_delay(Duration::from_millis(250))
            .max_delay(Duration::from_secs(30))
            .jitter(true)
            .on_status(429)
            .on_status(500)
            .on_status(502)
            .on_status(503)
            .on_status(504),
    )
    .middleware(DefaultHeaders::new().user_agent("my-crawler/1.0"))
    .middleware(StatusRetry::new())
    .fingerprint_policy(FingerprintPolicy::default().strip_tracking_params(true))
    .frontier(frontier)
    .metrics_interval(Duration::from_secs(30))
    .stats_checkpoint("crawl-report.checkpoint.json")
    .store(JsonlStore::new("items.jsonl")?)
    .store_buffer(1_000, 100)
    .run(MySpider)
    .await;

let stats = match result {
    Ok(stats) => stats,
    Err(err) => {
        tracing::error!(error = %err, "crawl failed before final report");
        return Err(err);
    }
};

let report = CrawlReport::from(stats);
std::fs::write("crawl-report.json", report.to_json_string_pretty())
    .map_err(|e| KumoError::store("write crawl report", e))?;
```

This is the same pattern used by the
[`production_crawler.rs`](../examples.md)
example: HTTP first, robots enabled, per-domain politeness, retry timing handled
by the scheduler, a persistent frontier, JSONL output, a bounded store buffer,
and a final `CrawlReport` after a successful run. A failed run returns its error
instead; it does not provide the final `CrawlStats` needed to build a report.

## Scope The Spider

Production spiders should define crawl boundaries in the spider, not only in
the infrastructure around it:

```rust
impl Spider for MySpider {
    type Item = MyItem;

    fn name(&self) -> &str {
        "my-spider"
    }

    fn allowed_domains(&self) -> Vec<&str> {
        vec!["example.com"]
    }

    fn max_depth(&self) -> Option<usize> {
        Some(5)
    }

    // start_urls() and parse() omitted
}
```

Use `allowed_domains()` to prevent accidental off-site crawls. Use
`max_depth()` when following arbitrary links. Use `max_pages()`, `max_items()`,
`max_errors()`, or `max_duration()` on the engine when a crawl job needs a hard
budget.

## Choose Concurrency Conservatively

Global concurrency is only useful when the scheduler can run enough eligible
requests. For one public domain, the real limit is usually
`PolitenessPolicy::per_domain_concurrency()` plus the per-domain delay and any
robots crawl delay. `timings.fetch` measures only the configured fetcher after a
request becomes eligible; it does not include scheduler or politeness waiting.
Use throughput, configured delays, and scheduler logs alongside reports before
raising concurrency.

Avoid sleeping inside `parse()` or middleware for crawl pacing. Let
`PolitenessPolicy`, `RetryPolicy`, and `StatusRetry` control scheduling so
workers are not held idle during retry backoff.

## Pick Durable Output

`JsonlStore` is a good default for production jobs because every accepted item
is appended as one JSON object per line. For slow stores, enable
`store_buffer(queue_capacity, batch_size)` so request tasks see bounded
backpressure instead of unbounded memory growth.

Database stores are useful when the crawl output must land directly in a
database, but they can become the limiting stage. When reports show high
`store.queue_full_waits`, `store.queue_wait`, or backend write latency, reduce
crawl concurrency, increase batch size, or write JSONL and bulk-load later.

## Keep Reports And State

Keep these artifacts together for each production run:

| Artifact | Why keep it |
| --- | --- |
| Scraped output | The data produced by the crawl |
| `crawl-report.json` | Final counters, rates, retry summary, timings, store stats, and stop reason for a successful run |
| Checkpoint report | Last known state if the process is interrupted |
| Frontier directory or Redis keys | Pending, seen, leased, delayed, and dead-letter request state for durable frontiers |
| Returned error, structured logs, and backend telemetry | Failure context when the run cannot produce a final report |

For short jobs, writing only the final report may be enough. For long-running
jobs, `stats_checkpoint(...)` gives you a periodically refreshed report
snapshot while the crawl is still running.

If a buffered store fails during the final flush, `run()` returns the store
error before buffered counters are copied into final stats. Diagnose that run
from the returned error, any `store.buffer_error` log emitted for an earlier
background batch failure, and the store backend's logs or telemetry. Do not
expect a final `CrawlReport` for that failed run.

## Disable Development Helpers

The HTTP cache is useful while developing selectors, but production crawls that
need fresh data should not use it. Cached responses can bypass normal fetch
latency and make rate, retry, and freshness signals misleading.

Browser fallback is useful when only some pages require JavaScript rendering.
If `browser_fallbacks` are common in the final report, size the job for browser
cost rather than HTTP cost.
