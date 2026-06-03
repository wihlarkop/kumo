---
description: Extend kumo crawls with async lifecycle hooks for metrics, audit logging, persistence, and policy enforcement.
---

# Crawl Hooks

Crawl hooks are async extension points that run inside the crawl engine when
typed crawl lifecycle events happen. Use hooks when your application needs to
attach behavior to a crawl, such as metrics counters, audit logs, custom
persistence, alerts, or policy checks.

Hooks use the same `CrawlEvent` model as the event stream. The difference is
that hooks run in-process and can affect crawl success when configured to abort
on hook failures.

## Quick Start

```rust
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use kumo::prelude::*;

#[derive(Default, Clone)]
struct MetricsHook {
    completed: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl CrawlHook for MetricsHook {
    async fn on_request_completed(&self, event: &CrawlEvent) -> Result<(), KumoError> {
        if let CrawlEvent::RequestCompleted { url, status, .. } = event {
            self.completed.fetch_add(1, Ordering::Relaxed);
            tracing::info!(%url, status, "request completed");
        }
        Ok(())
    }
}

CrawlEngine::builder()
    .hook(MetricsHook::default())
    .run(MySpider)
    .await?;
```

## Lifecycle Methods

Implement `CrawlHook` and override only the methods you need. Every method is a
no-op by default.

| Method | Event |
| --- | --- |
| `on_crawl_started` | `CrawlEvent::CrawlStarted` |
| `on_request_scheduled` | `CrawlEvent::RequestScheduled` |
| `on_request_skipped` | `CrawlEvent::RequestSkipped` |
| `on_request_started` | `CrawlEvent::RequestStarted` |
| `on_request_completed` | `CrawlEvent::RequestCompleted` |
| `on_request_retried` | `CrawlEvent::RequestRetried` |
| `on_request_failed` | `CrawlEvent::RequestFailed` |
| `on_task_panicked` | `CrawlEvent::TaskPanicked` |
| `on_item_scraped` | `CrawlEvent::ItemScraped` |
| `on_item_dropped` | `CrawlEvent::ItemDropped` |
| `on_crawl_finished` | `CrawlEvent::CrawlFinished` |

You can also override `on_event` when one handler should see every event:

```rust
#[async_trait::async_trait]
impl CrawlHook for AuditHook {
    async fn on_event(&self, event: &CrawlEvent) -> Result<(), KumoError> {
        tracing::info!(event = event.name(), "crawl event");
        Ok(())
    }
}
```

## Registering Hooks

Hooks are registered on `CrawlEngine`:

```rust
CrawlEngine::builder()
    .hook(MetricsHook::default())
    .hook(AuditHook)
    .run(MySpider)
    .await?;
```

Hooks run in registration order. If multiple hooks are registered, Kumo calls
the first hook, then the second hook, and so on for each event.

## Error Policy

By default, hook failures are logged and the crawl continues:

```rust
CrawlEngine::builder()
    .hook(MyHook)
    .hook_error_policy(HookErrorPolicy::LogAndContinue)
    .run(MySpider)
    .await?;
```

Use `AbortCrawl` when hook behavior is required for correctness:

```rust
CrawlEngine::builder()
    .hook(RequiredAuditHook)
    .hook_error_policy(HookErrorPolicy::AbortCrawl)
    .run(MySpider)
    .await?;
```

With `AbortCrawl`, the first hook error returns from the engine as
`KumoErrorKind::Hook`. The error message includes the event label that failed.

## Events Or Hooks

Use crawl events when another task or embedded application wants a best-effort
stream of crawl activity:

```rust
let (engine, mut events) = CrawlEngine::builder().event_channel(1024);
```

Use crawl hooks when the crawl should run code at lifecycle points:

```rust
CrawlEngine::builder().hook(MetricsHook::default());
```

You can use both together. Kumo sends the `CrawlEvent` to the broadcast channel
and then dispatches it to registered hooks.

## Production Notes

- Keep hook work fast. Heavy database writes or network calls should use
  buffering when throughput matters.
- Prefer `LogAndContinue` for observability-only hooks.
- Use `AbortCrawl` for required side effects, such as mandatory audit writes.
- Hooks run inside crawl tasks, so slow hooks can reduce crawl throughput.
- `run_all()` events include `spider_index: Some(index)` for per-spider
  attribution.

## Example

See the [`crawl_hooks.rs`](../examples.md) entry for a complete runnable example
using `MockFetcher`.
