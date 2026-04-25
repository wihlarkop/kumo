# Item Stream API

`CrawlEngine::stream()` yields items in real time as they are scraped, as an async `Stream`. This lets you process items while the crawl is still running — useful for streaming to Kafka, WebSockets, databases, or any consumer that benefits from low latency.

## Basic Usage

```rust
use kumo::prelude::*;

let mut stream = CrawlEngine::builder()
    .concurrency(4)
    .stream(MySpider)
    .await?;

while let Some(item) = stream.next().await {
    // item is serde_json::Value
    println!("{}", serde_json::to_string_pretty(&item)?);
}
```

The crawl runs in a background Tokio task. The `stream.next()` call blocks until an item is available or the crawl finishes.

## Backpressure

The stream has a bounded internal channel. When the buffer is full, the crawler pauses until the consumer catches up — providing natural backpressure.

```rust
// Default buffer: 100 items
CrawlEngine::builder()
    .stream_buffer(10)   // pause the crawler when 10 items are buffered
    .stream(MySpider)
    .await?;
```

Use a smaller buffer when the consumer is slow (e.g. writing to a remote API). Use a larger buffer when the consumer has bursty throughput.

## Stopping Early

Dropping the stream stops the crawl gracefully:

```rust
let mut stream = CrawlEngine::builder()
    .stream(MySpider)
    .await?;

let mut count = 0;
while let Some(item) = stream.next().await {
    process(item).await;
    count += 1;
    if count >= 1000 {
        break;  // drop stream here — crawl stops
    }
}
```

## Combining with Middleware and Pipelines

`.stream()` supports the full engine builder API:

```rust
CrawlEngine::builder()
    .concurrency(8)
    .retry(3, Duration::from_millis(200))
    .middleware(DefaultHeaders::new().user_agent("kumo/0.1"))
    .pipeline(RequireFields::new(&["title", "url"]))
    .stream_buffer(50)
    .stream(MySpider)
    .await?;
```

!!! note
    `.store()` is ignored when using `.stream()` — items go to the stream, not the store.

## When to Use Stream vs Store

| | `.run()` + store | `.stream()` |
|---|---|---|
| File output (JSONL, CSV) | ✅ | ❌ unnecessary |
| Real-time processing | ❌ | ✅ |
| Kafka / WebSocket push | ❌ | ✅ |
| Stop after N items | ❌ complex | ✅ drop the stream |
| Parallel consumers | ❌ | ✅ use `tokio::spawn` |
