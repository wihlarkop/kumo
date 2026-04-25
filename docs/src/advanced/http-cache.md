# HTTP Cache

kumo can cache HTTP responses to disk, skipping real network requests when the same URL is fetched again. Useful during development to avoid hammering sites while iterating on `parse()` logic.

## Usage

```rust
CrawlEngine::builder()
    .http_cache("./cache")          // cache responses in ./cache directory
    .run(MySpider)
    .await?;
```

Responses are stored by URL hash. On subsequent runs, cached responses are served from disk instantly.

## TTL

Set a maximum cache age:

```rust
CrawlEngine::builder()
    .http_cache("./cache")
    .cache_ttl(Duration::from_secs(60 * 60))   // expire entries after 1 hour
    .run(MySpider)
    .await?;
```

Expired entries are refetched and the cache is updated.

## When to Use

- **Development** — iterate on selectors without network requests
- **Re-processing** — re-run `parse()` logic on already-fetched pages
- **Rate-limited targets** — reduce the number of live requests

!!! warning
    Do not use the HTTP cache in production crawls that need fresh data — cached responses bypass your crawl delay and auto-throttle.
