---
description: Subscribe to typed kumo crawl lifecycle events for dashboards, progress bars, alerts, and embedded crawl runners.
---

# Crawl Events

Kumo exposes typed crawl lifecycle events through `CrawlEvent`. Use events when
your application needs programmatic visibility while a crawl is running, such as
dashboards, progress bars, alerts, custom metrics, or embedded crawl runners.

Events complement structured `tracing` logs, OpenTelemetry, `CrawlStats`, and
`CrawlReport`. They do not replace logging.

## Quick Start

Use `.event_channel(capacity)` when you want Kumo to create the event channel:

```rust
use kumo::prelude::*;

let (engine, mut events) = CrawlEngine::builder().event_channel(1024);

let listener = tokio::spawn(async move {
    while let Ok(event) = events.recv().await {
        match event {
            CrawlEvent::RequestCompleted { url, status, items, .. } => {
                tracing::info!(%url, status, items, "page completed");
            }
            CrawlEvent::CrawlFinished { report, .. } => {
                tracing::info!(
                    pages = report.pages_crawled,
                    items = report.items_scraped,
                    "crawl finished"
                );
                break;
            }
            _ => {}
        }
    }
});

engine.run(MySpider).await?;
listener.await?;
```

Use `.events(tx)` when your application owns the broadcast channel:

```rust
use kumo::{engine::CrawlEngine, events::CrawlEvent};

let (tx, mut rx) = tokio::sync::broadcast::channel::<CrawlEvent>(1024);

CrawlEngine::builder()
    .events(tx)
    .run(MySpider)
    .await?;
```

## Delivery Guarantees

Event delivery is best-effort:

- If there are no receivers, the crawl continues.
- If a receiver lags, the crawl continues.
- Event send errors are ignored by the engine.
- Events are for observability, not crawl correctness.

Choose a channel capacity large enough for your listener. For high-throughput
crawls, drain events in a dedicated task and aggregate them outside the engine.

## Event Model

`CrawlEvent` variants describe user-meaningful lifecycle points:

| Event | Meaning |
| --- | --- |
| `CrawlStarted` | A spider started and its seed URL count is known. |
| `RequestScheduled` | A request entered the scheduler/frontier. |
| `RequestSkipped` | A request was not fetched because it was duplicate, blocked by robots.txt, over depth, or outside allowed domains. |
| `RequestStarted` | A worker started processing a request. |
| `RequestCompleted` | A request fetched, parsed, and stored its accepted items. |
| `RequestRetried` | A request was scheduled for retry. |
| `RequestFailed` | A request reached the spider error policy path. |
| `TaskPanicked` | A worker task panicked before returning a normal request result. |
| `ItemScraped` | An item passed pipelines and was handed to the store. |
| `ItemDropped` | A pipeline filtered an item or returned an error. |
| `CrawlFinished` | A spider finished and includes a final `CrawlReport`. |

Every event includes the spider name. Events emitted by `run_all()` also include
`spider_index: Some(index)` so applications can separate multiple spiders in one
engine run. Single-spider `run()` events use `spider_index: None`.

## Stable Event Labels

Use `CrawlEvent::name()` when you need a stable label for dashboards, counters,
or metrics tags:

```rust
while let Ok(event) = events.recv().await {
    tracing::debug!(event = event.name(), "crawl event");
}
```

Current labels:

| Event | Label |
| --- | --- |
| `CrawlStarted` | `crawl_started` |
| `RequestScheduled` | `request_scheduled` |
| `RequestSkipped` | `request_skipped` |
| `RequestStarted` | `request_started` |
| `RequestCompleted` | `request_completed` |
| `RequestRetried` | `request_retried` |
| `RequestFailed` | `request_failed` |
| `TaskPanicked` | `task_panicked` |
| `ItemScraped` | `item_scraped` |
| `ItemDropped` | `item_dropped` |
| `CrawlFinished` | `crawl_finished` |

## Ordering Expectations

Events are emitted from the same logical points as the engine's structured logs.
For one successful request, consumers should generally see:

1. `RequestScheduled`
2. `RequestStarted`
3. zero or more `ItemScraped` or `ItemDropped` events
4. `RequestCompleted`

Retries emit `RequestRetried` before the retry is requeued. Permanent failures
emit `RequestFailed`. A crawl emits `CrawlFinished` after final stats and the
stop reason are known.

Concurrent crawls can interleave events from different requests or spiders. Use
`spider`, `spider_index`, `url`, `depth`, and `attempt` fields to correlate
events instead of assuming global ordering.

## Skip And Drop Reasons

`RequestSkipped` carries a typed `RequestSkipReason`:

| Reason | Meaning |
| --- | --- |
| `RobotsTxt` | `robots.txt` blocked the request. |
| `Duplicate` | The fingerprint/frontier already saw the request. |
| `DepthLimit` | The request exceeded `Spider::max_depth()`. |
| `DomainDenied` | The request was outside `Spider::allowed_domains()`. |

`ItemDropped` carries a typed `ItemDropReason`:

| Reason | Meaning |
| --- | --- |
| `PipelineFiltered` | A pipeline returned `Ok(None)`. |
| `PipelineError` | A pipeline returned `Err(_)`; `error_kind` is included. |

Both reason enums expose `as_str()` for stable snake_case labels.

## Relationship To Logs And Reports

| Surface | Purpose |
| --- | --- |
| `tracing` | Human-readable logs, JSON logs, OpenTelemetry spans and metrics. |
| `CrawlEvent` | Programmatic lifecycle hooks while the crawl is running. |
| `CrawlStats` | Final counters returned by the engine. |
| `CrawlReport` | Cloneable final report with derived rates and JSON export helpers. |

Use events for live application behavior. Use `tracing` for logs and telemetry.
Use `CrawlReport` when you need a durable final summary.

## Example

Run the included no-network example:

```bash
cargo run --example crawl_events
```

It uses `MockFetcher`, subscribes to events with `.event_channel(128)`, and
prints request completion plus final crawl totals.
