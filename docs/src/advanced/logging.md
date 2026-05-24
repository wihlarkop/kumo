# Logging

Kumo emits structured [`tracing`](https://docs.rs/tracing) events. Applications
choose how to collect and format those events by installing a tracing
subscriber. Kumo does not install a subscriber for normal library use.

Most examples use `tracing_subscriber::fmt()`:

```rust
tracing_subscriber::fmt()
    .with_env_filter(
        std::env::var("RUST_LOG")
            .unwrap_or_else(|_| "kumo::crawl=info,kumo::request=info".into()),
    )
    .init();
```

## Recommended Filters

For normal production runs:

```bash
RUST_LOG=kumo::crawl=info,kumo::request=info
```

For debugging scheduling, cache, pipeline, or item-drop behavior:

```bash
RUST_LOG=kumo=debug
```

For quiet application logs with only final crawl summaries:

```bash
RUST_LOG=kumo::crawl=info,kumo::request=warn
```

## Event Targets

Kumo uses stable tracing targets for important runtime areas:

| Target | Events |
|--------|--------|
| `kumo::crawl` | Crawl start, periodic metrics, interruption, abort, completion |
| `kumo::request` | Request retries, skips, robots-blocked requests, rate-limit waits |
| `kumo::item` | Item drops and pipeline drop errors |
| `kumo::cache` | HTTP cache hits, misses, bypasses, and skipped cache writes |

Common event names include `crawl.start`, `crawl.metrics`, `crawl.complete`,
`request.retry`, `request.retry_exhausted`, `request.skip`,
`request.robots_blocked`, `item.drop`, `cache.hit`, and `cache.miss`.

## JSON Logs

Use JSON logs when sending crawl output to systems such as Datadog, Loki,
CloudWatch, or Vector:

```rust
tracing_subscriber::fmt()
    .json()
    .with_env_filter(
        std::env::var("RUST_LOG")
            .unwrap_or_else(|_| "kumo::crawl=info,kumo::request=info".into()),
    )
    .init();
```

Enable the `json` feature on `tracing-subscriber` in your application:

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
```

For OpenTelemetry export, enable Kumo's `otel` feature and see
[OpenTelemetry](otel.md).

## Library Boundary

Kumo logs with `tracing` but does not own the logging backend. This keeps the
framework composable inside CLIs, services, cron jobs, and larger applications.
If you need programmatic lifecycle hooks instead of logs, use the current
`CrawlStats` and `CrawlReport` APIs; a typed event/signal system is planned as
separate future work.
