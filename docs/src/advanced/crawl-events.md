# Crawl Events Design

Kumo already exposes operational visibility through structured `tracing` logs,
OpenTelemetry, `CrawlStats`, and `CrawlReport`. The next observability layer is
a typed event surface for applications that need programmatic hooks.

This page describes the planned design. It is not a stable public API yet.

## Goals

- Let applications subscribe to crawl lifecycle events without parsing logs.
- Keep event payloads typed and Rust-native.
- Support dashboards, progress bars, custom alerts, and embedded crawl runners.
- Preserve `tracing` as the logging and telemetry surface.
- Avoid a Scrapy-style dynamic signal system that hides type errors until
  runtime.

## Non-Goals

- Do not replace `tracing`.
- Do not add a custom logger trait.
- Do not require all users to pay for event delivery when they do not subscribe.
- Do not expose internal scheduler types as public API before they are stable.

## Candidate Event Model

A future `CrawlEvent` enum should describe user-meaningful events with stable
fields:

```rust
pub enum CrawlEvent {
    CrawlStarted {
        spider: String,
    },
    RequestScheduled {
        spider: String,
        url: String,
        depth: usize,
    },
    RequestSkipped {
        spider: String,
        url: String,
        reason: SkipReason,
    },
    RequestCompleted {
        spider: String,
        url: String,
        status: u16,
        bytes: u64,
        elapsed_ms: u128,
    },
    RequestFailed {
        spider: String,
        url: String,
        kind: KumoErrorKind,
    },
    RequestRetried {
        spider: String,
        url: String,
        attempt: u32,
        delay_ms: u128,
    },
    ItemScraped {
        spider: String,
    },
    ItemDropped {
        spider: String,
        reason: DropReason,
    },
    CrawlFinished {
        spider: String,
        report: CrawlReport,
    },
}
```

The exact names and payloads can change before implementation. The important
constraint is that event variants should be stable enough for application code
to match on without relying on display strings.

## Subscription API

The preferred first API is a Tokio broadcast channel owned by the application:

```rust
let (tx, mut rx) = tokio::sync::broadcast::channel(1024);

let stats = CrawlEngine::builder()
    .events(tx)
    .run(MySpider)
    .await?;
```

This keeps the engine async-native and avoids callback lifetime complexity. A
dashboard or progress task can receive events independently:

```rust
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        match event {
            CrawlEvent::RequestCompleted { url, status, .. } => {
                tracing::debug!(%url, status, "request completed");
            }
            CrawlEvent::CrawlFinished { report, .. } => {
                tracing::info!(
                    pages = report.pages_crawled,
                    items = report.items_scraped,
                    "crawl finished"
                );
            }
            _ => {}
        }
    }
});
```

If the channel has no receivers or is full, the engine should not fail the
crawl. Event delivery is observability, not crawl correctness.

## Relationship To Logging

Events and logs serve different purposes:

| Surface | Purpose |
|---------|---------|
| `tracing` | Human-readable logs, JSON logs, OpenTelemetry spans and metrics |
| `CrawlReport` | Final durable crawl summary |
| `CrawlEvent` | Programmatic lifecycle hooks while the crawl is running |

Kumo should emit both logs and events from the same logical points in the engine,
but they should remain separate APIs. Applications should not need to install a
custom logger to build a dashboard.

## Implementation Notes

- Add a new `events` module with public event types.
- Store an optional event sender in `CrawlEngineBuilder`.
- Clone the sender into task contexts only when configured.
- Use best-effort sends and ignore receiver lag as a subscriber concern.
- Include tests that prove events are emitted for success, retry, failure, item
  drop, and crawl finish.
- Document event payload stability before release.

The event bus is large enough to deserve a minor release when implemented.
