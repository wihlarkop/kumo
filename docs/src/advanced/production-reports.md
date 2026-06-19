# Production Reports

Kumo returns `CrawlStats` from every crawl. Convert it into `CrawlReport` when a
production job needs a stable summary for logs, dashboards, alerting, or files:

```rust
let stats = CrawlEngine::builder()
    .run(MySpider)
    .await?;

let report = CrawlReport::from(stats);
std::fs::write("crawl-report.json", report.to_json_string_pretty())?;
```

Reports keep the raw counters and derived health signals together. The raw
counters tell you what happened; the derived helpers make common alerts easy to
write without duplicating rate math in every crawler.

For long-running crawls, enable checkpointed reports so Kumo periodically writes
the current report snapshot and overwrites it again when the crawl finishes:

```rust
let stats = CrawlEngine::builder()
    .stats_checkpoint("crawl-report.checkpoint.json")
    .run(MySpider)
    .await?;
```

Use a custom interval when the default 30-second cadence is too frequent or too
slow:

```rust
let stats = CrawlEngine::builder()
    .stats_checkpoint_interval(
        "crawl-report.checkpoint.json",
        std::time::Duration::from_secs(300),
    )
    .run(MySpider)
    .await?;
```

The checkpoint file uses the same JSON shape as `CrawlReport::to_json_value()`.
For `run_all()`, Kumo writes a JSON array with one report per spider.

## Health Signals

| Field or helper | Use it for |
|-----------------|------------|
| `pages_crawled` | Completed response count |
| `items_scraped` | Extracted item volume |
| `errors` | Permanent request, parse, store, or task failures |
| `error_kinds` | Grouping failures by stable `KumoErrorKind` labels |
| `scheduled` | Requests accepted by the scheduler |
| `deduped` | Requests dropped by fingerprint deduplication |
| `retries` | Retry attempts scheduled by retry policy or spider error policy |
| `retry_exhausted` | Requests that still failed after retry capacity was used |
| `retry_summary()` | Retry attempts, exhausted retries, retry pressure, exhaustion, and exhausted-failure rates |
| `browser_fallbacks` | Requests retried through the browser fallback path |
| `browser_fallback_successes` | Browser fallback attempts that returned a rendered response |
| `browser_fallback_failures` | Browser fallback attempts that failed and kept the original HTTP response |
| `robots_blocked` | Requests blocked by robots.txt |
| `stop_reason` | Why the crawl stopped |
| `pages_per_second()` | Crawl throughput |
| `items_per_second()` | Extraction throughput |
| `bytes_per_second()` | Download throughput |
| `error_rate()` | Permanent failures divided by completed and failed requests |
| `success_rate()` | Completed requests divided by completed and failed requests |
| `retry_exhaustion_rate()` | Retry-exhausted requests divided by retry attempts |
| `timings` | Cumulative successful-request phase timings for bottleneck diagnosis |
| `store` | Store-buffer queue/write counters when `store_buffer()` is enabled |

The JSON export uses the same names in snake_case, including derived fields such
as `pages_per_second`, `error_rate`, `retry_exhaustion_rate`,
`retry_summary`, `timings`, and `store`.

When `browser_fallback(...)` is enabled, the browser fallback counters help show
how much of the crawl required JavaScript rendering. A high fallback rate usually
means browser rendering is part of the target's normal path and crawl
concurrency should be sized for browser cost, not HTTP cost.

## Store Summary

`CrawlReport::store` is always present. When `store_buffer()` is not enabled,
its counters are zero and `buffered` is `false`. When buffering is enabled, it
shows how much item output moved through the bounded writer:

| Field | Meaning |
|-------|---------|
| `buffered` | Whether the bounded store buffer was enabled |
| `queued` | Items accepted into the store queue |
| `written` | Items written by the background writer |
| `batches` | Non-empty batch write calls |
| `failed_writes` | Items in batches that returned a store error |
| `failed_batches` | Non-empty batch write calls that returned a store error |
| `queue_full_waits` | Times request tasks observed a full store queue |
| `queue_wait` | Total time request tasks spent waiting for queue capacity |
| `queue_wait_max` | Longest queue wait for one accepted item |
| `average_queue_wait_per_item()` | Average queue wait per accepted item |
| `write` | Total time the writer spent inside store write attempts |
| `write_max` | Longest single batch write attempt |
| `average_write_per_batch()` | Average write latency per batch attempt |
| `average_write_per_item()` | Average write latency per written or failed item |

If `queue_full_waits` or `queue_wait` is high, the item store is backpressuring
the crawl. That is often better than unbounded memory growth, but it means
throughput is now limited by item persistence.

The JSON export includes millisecond and second fields for totals, averages,
and maxes:

| JSON field | Meaning |
|------------|---------|
| `queue_wait_ms`, `queue_wait_secs` | Total enqueue wait |
| `queue_wait_avg_ms`, `queue_wait_avg_secs` | Average enqueue wait per item |
| `queue_wait_max_ms`, `queue_wait_max_secs` | Maximum enqueue wait for one item |
| `write_ms`, `write_secs` | Total backend write-attempt time |
| `write_avg_batch_ms`, `write_avg_batch_secs` | Average backend write time per batch attempt |
| `write_avg_item_ms`, `write_avg_item_secs` | Average backend write time per written or failed item |
| `write_max_ms`, `write_max_secs` | Maximum backend write time for one batch attempt |

Buffered stores use `StoreFailurePolicy::Abort` by default. On the first
background store error, Kumo records the failed batch counters and reports the
store error instead of continuing with possible item loss.

## Retry Summary

`CrawlReport::retry_summary()` groups retry health into one small report:

| Field | Meaning |
|-------|---------|
| `attempts` | Total retry attempts that were scheduled |
| `exhausted` | Requests that still failed after retry capacity was used |
| `pressure_rate` | Retry attempts divided by scheduled requests |
| `exhaustion_rate` | Exhausted retries divided by retry attempts |
| `exhausted_failure_rate` | Exhausted retries divided by permanent errors |

Use the fields together:

- high `pressure_rate` means the target or network is making many requests
  retry, even if the crawl eventually recovers;
- high `exhaustion_rate` means retries are often not helping;
- high `exhausted_failure_rate` means permanent failures are mostly retry
  exhaustion, which often points to rate limits, blocking, or sustained upstream
  instability.

The JSON report includes the same values under `retry_summary`, while keeping
the top-level `retries`, `retry_exhausted`, and `retry_exhaustion_rate` fields
for compatibility.

## Timing Breakdown

`CrawlReport::timings` splits successful request work into broad phases:

| Field | Measures |
|-------|----------|
| `middleware_request` | Time spent in `Middleware::before_request` |
| `fetch` | Time spent waiting for the configured fetcher |
| `middleware_response` | Time spent in `Middleware::after_response` |
| `parse` | Time spent in the spider `parse` method |
| `pipeline` | Time spent in item pipelines |
| `store` | Time spent writing accepted items to the item store |

These are cumulative task timings, not exclusive wall-clock percentages. In a
concurrent crawl, the sum can be higher than `duration` because many requests
run at the same time. Use the largest phase as a direction signal: high `fetch`
usually points to target latency or politeness limits, high `parse` points to
selector/extraction work, and high `store` points to output backpressure.

## Alert Examples

Use `error_rate()` for broad crawl health. A nonzero error rate is normal on the
open web, but a sudden increase usually means the target changed, credentials
expired, the crawler is being blocked, or a store is failing.

```rust
if report.error_rate() > 0.10 {
    tracing::warn!(
        error_rate = report.error_rate(),
        errors = report.errors,
        pages = report.pages_crawled,
        "crawl error rate exceeded threshold"
    );
}
```

Use `retry_summary()` when retry attempts are happening. This separates retry
pressure from retry exhaustion:

```rust
let retry = report.retry_summary();

if retry.attempts > 0 && retry.exhaustion_rate > 0.25 {
    tracing::warn!(
        retry_exhaustion_rate = retry.exhaustion_rate,
        retry_pressure_rate = retry.pressure_rate,
        retry_exhausted_failure_rate = retry.exhausted_failure_rate,
        retries = retry.attempts,
        retry_exhausted = retry.exhausted,
        "retry exhaustion exceeded threshold"
    );
}
```

Use `pages_per_second()` and `items_per_second()` for throughput alerts. Compare
them against your own historical baseline instead of a universal threshold.

## Domain Breakdowns

`domains` contains per-domain scheduler and failure counters. Use it when one
domain is unhealthy but the whole crawl still looks fine:

```rust
for (domain, stats) in &report.domains {
    if stats.failed > 0 && stats.completed == 0 {
        tracing::warn!(
            domain,
            failed = stats.failed,
            error_kinds = ?stats.error_kinds,
            "domain had failures and no completed pages"
        );
    }
}
```

Per-domain reports are especially useful for multi-domain crawls where one
target can be blocked, slow, or unavailable without stopping the entire job.

## Operational Pattern

For scheduled production crawls, write the report next to your scraped output and
send the same values to logs or metrics:

```rust
let report = CrawlReport::from(stats);
let retry = report.retry_summary();

tracing::info!(
    pages = report.pages_crawled,
    items = report.items_scraped,
    errors = report.errors,
    error_rate = report.error_rate(),
    pages_per_second = report.pages_per_second(),
    retry_pressure_rate = retry.pressure_rate,
    retry_exhaustion_rate = retry.exhaustion_rate,
    retry_exhausted_failure_rate = retry.exhausted_failure_rate,
    store_buffered = report.store.buffered,
    store_queued = report.store.queued,
    store_written = report.store.written,
    store_failed_writes = report.store.failed_writes,
    store_queue_full_waits = report.store.queue_full_waits,
    store_queue_wait_avg_secs = report.store.average_queue_wait_per_item().as_secs_f64(),
    store_write_avg_batch_secs = report.store.average_write_per_batch().as_secs_f64(),
    fetch_secs = report.timings.fetch.as_secs_f64(),
    parse_secs = report.timings.parse.as_secs_f64(),
    store_secs = report.timings.store.as_secs_f64(),
    stop_reason = report.stop_reason.map(StopReason::as_str),
    "crawl report"
);

std::fs::write("crawl-report.json", report.to_json_string_pretty())?;
```

Keep the JSON report as the durable audit record. Use structured logs for live
operations and OpenTelemetry when you need centralized metrics or traces.

With the `otel` feature enabled and `kumo::otel::init()` active, Kumo records
the same production report counters as OTLP metrics at crawl completion. It also
records successful request fetch latency as a histogram during the crawl. See
[OpenTelemetry](otel.md#production-metrics) for metric names and attributes.
