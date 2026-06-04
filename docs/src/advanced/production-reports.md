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
| `robots_blocked` | Requests blocked by robots.txt |
| `stop_reason` | Why the crawl stopped |
| `pages_per_second()` | Crawl throughput |
| `items_per_second()` | Extraction throughput |
| `bytes_per_second()` | Download throughput |
| `error_rate()` | Permanent failures divided by completed and failed requests |
| `success_rate()` | Completed requests divided by completed and failed requests |
| `retry_exhaustion_rate()` | Retry-exhausted requests divided by retry attempts |
| `timings` | Cumulative successful-request phase timings for bottleneck diagnosis |

The JSON export uses the same names in snake_case, including derived fields such
as `pages_per_second`, `error_rate`, `retry_exhaustion_rate`, and `timings`.

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

Use `retry_exhaustion_rate()` when retry attempts are happening but not helping.
This usually points to sustained upstream failures, rate limits, or blocking:

```rust
if report.retries > 0 && report.retry_exhaustion_rate() > 0.25 {
    tracing::warn!(
        retry_exhaustion_rate = report.retry_exhaustion_rate(),
        retries = report.retries,
        retry_exhausted = report.retry_exhausted,
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

tracing::info!(
    pages = report.pages_crawled,
    items = report.items_scraped,
    errors = report.errors,
    error_rate = report.error_rate(),
    pages_per_second = report.pages_per_second(),
    retry_exhaustion_rate = report.retry_exhaustion_rate(),
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
