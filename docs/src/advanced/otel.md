# OpenTelemetry

The `otel` feature exports kumo traces and production metrics to any
OpenTelemetry-compatible backend via OTLP/gRPC - Jaeger, Grafana Tempo,
Datadog, Honeycomb, and others.

No changes to spider code are required. Every request, retry, item scrape, and
pipeline drop is automatically traced with structured fields.

## Installation

```toml
kumo = { version = "0.2", features = ["otel"] }
```

## Usage

Call `kumo::otel::init()` once at the start of `main`, before creating any
`CrawlEngine`:

```rust
#[tokio::main]
async fn main() -> Result<(), kumo::error::KumoError> {
    kumo::otel::init("my-crawler", "http://localhost:4317").await?;

    CrawlEngine::builder()
        .concurrency(8)
        .run(MySpider)
        .await?;

    kumo::otel::shutdown();  // flush remaining spans and metrics before exit
    Ok(())
}
```

| Parameter | Description |
|-----------|-------------|
| `service_name` | Identifies this process in your APM dashboard |
| `otlp_endpoint` | gRPC endpoint, e.g. `"http://localhost:4317"` |

`shutdown()` flushes all buffered spans and metrics. Always call it before
`main` returns.

## Local Testing with Jaeger

```bash
# Start an all-in-one Jaeger container
docker run -p 16686:16686 -p 4317:4317 jaegertracing/all-in-one

# Run a spider with otel and debug logging
RUST_LOG=kumo=debug cargo run --features otel --example books

# Open the Jaeger UI
open http://localhost:16686
```

## Log Level

OTel init registers the global `tracing` subscriber. Use `RUST_LOG` as normal:

```bash
RUST_LOG=kumo=debug,info cargo run --features otel
```

## What Is Traced

| Span / Event | Fields |
|---|---|
| HTTP request | `url`, `status`, `latency_ms`, `bytes` |
| Retry attempt | `url`, `attempt`, `error` |
| Item scraped | `spider`, `item_type` |
| Pipeline drop | `spider`, `stage`, `reason` |
| Frontier enqueue | `url`, `depth` |
| Robots.txt fetch | `domain`, `cached` |

## Production Metrics

When `kumo::otel::init()` is active, Kumo also exports production crawl metrics
through the same OTLP endpoint. Request, page, item, retry, error, robots, and
skip counters are recorded live while the crawl runs. Fetch latency and total
successful request duration are recorded as histograms per successful request.
Buffered store summary metrics are recorded from the final `CrawlReport`
snapshot because the final writer totals are only known after flush.

Live request metrics include `spider` and `domain`; multi-spider runs also
include `spider.index`. Skip counters include `skip.reason`; error counters
include `error.kind`; item-drop counters include `drop.reason` and include
`error.kind` for pipeline errors. Final store summary metrics include
`stop.reason` when available.

| Metric | Type | Source |
|--------|------|--------|
| `kumo.requests.scheduled` | Counter | Live scheduler accepts |
| `kumo.requests.deduped` | Counter | Live duplicate skips |
| `kumo.requests.skipped` | Counter | Live pre-fetch skips with `skip.reason` |
| `kumo.pages.crawled` | Counter | Live successful request completions |
| `kumo.items.scraped` | Counter | Live stored items |
| `kumo.items.dropped` | Counter | Live pipeline drops with `drop.reason` |
| `kumo.errors` | Counter | Live permanent request/task failures with `error.kind` |
| `kumo.retries` | Counter | Live retry attempts scheduled |
| `kumo.retries.exhausted` | Counter | Live retry-exhausted requests |
| `kumo.robots.blocked` | Counter | Live robots.txt blocks |
| `kumo.fetch.latency` | Histogram, seconds | Successful request fetch phase |
| `kumo.request.duration` | Histogram, seconds | Successful request processing time |
| `kumo.store.queued` | Counter | `CrawlReport::store.queued` |
| `kumo.store.written` | Counter | `CrawlReport::store.written` |
| `kumo.store.failed_writes` | Counter | `CrawlReport::store.failed_writes` |
| `kumo.store.failed_batches` | Counter | `CrawlReport::store.failed_batches` |
| `kumo.store.queue_full_waits` | Counter | `CrawlReport::store.queue_full_waits` |
| `kumo.store.queue_wait` | Histogram, seconds | Average queue wait per accepted item |
| `kumo.store.write` | Histogram, seconds | Average write time per batch attempt |

Store metrics are zero unless `CrawlEngine::store_buffer(...)` is enabled.
Store queue and write latency histograms currently record final crawl averages,
not every individual queue wait or backend write attempt.
