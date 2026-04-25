# OpenTelemetry

The `otel` feature exports all kumo spans and events to any OpenTelemetry-compatible backend via OTLP/gRPC — Jaeger, Grafana Tempo, Datadog, Honeycomb, and others.

No changes to spider code are required. Every request, retry, item scrape, and pipeline drop is automatically traced with structured fields.

## Installation

```toml
kumo = { version = "0.1", features = ["otel"] }
```

## Usage

Call `kumo::otel::init()` once at the start of `main`, before creating any `CrawlEngine`:

```rust
#[tokio::main]
async fn main() -> Result<(), kumo::error::KumoError> {
    kumo::otel::init("my-crawler", "http://localhost:4317").await?;

    CrawlEngine::builder()
        .concurrency(8)
        .run(MySpider)
        .await?;

    kumo::otel::shutdown();  // flush remaining spans before exit
    Ok(())
}
```

| Parameter | Description |
|-----------|-------------|
| `service_name` | Identifies this process in your APM dashboard |
| `otlp_endpoint` | gRPC endpoint, e.g. `"http://localhost:4317"` |

`shutdown()` flushes all buffered spans. Always call it before `main` returns.

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
