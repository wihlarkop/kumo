# Spiders

A spider is a struct that implements the `Spider` trait. It tells kumo where to start, how to parse each page, and what items to emit.

## The Spider Trait

```rust
#[async_trait::async_trait]
pub trait Spider: Send + Sync {
    type Item: serde::Serialize + Send;

    fn name(&self) -> &str;
    fn start_urls(&self) -> Vec<String>;

    async fn parse(
        &self,
        response: &Response,
    ) -> Result<Output<Self::Item>, KumoError>;

    // --- Optional hooks ---

    /// Called once before the crawl starts.
    async fn open(&self) -> Result<(), KumoError> { Ok(()) }

    /// Called once after the crawl finishes.
    async fn close(&self, stats: &CrawlStats) -> Result<(), KumoError> { Ok(()) }

    /// Only crawl these domains (empty = no restriction).
    fn allowed_domains(&self) -> Vec<&str> { vec![] }

    /// Stop following links deeper than this.
    fn max_depth(&self) -> Option<usize> { None }

    /// How to handle a fetch/parse error for a URL.
    fn on_error(&self, _url: &str, _err: &KumoError) -> ErrorPolicy {
        ErrorPolicy::Skip
    }
}
```

## Output

`parse()` returns `Output<T>` — a builder that collects items and requests to follow:

```rust
Output::new()
    .item(my_item)                    // add one item
    .items(vec![a, b, c])             // add many items
    .follow("https://next-page")      // enqueue a GET request
    .follow_many(links)               // enqueue many GET requests
    .request(
        CrawlRequest::get("https://example.com/high-priority")
            .priority(10)
            .dont_filter(true),
    )
```

Items are serialized to JSON exactly once and passed to pipelines and the store.
Use `CrawlRequest` when a follow-up request needs custom priority, headers,
method/body, metadata, or duplicate filtering behavior.

## Lifecycle Hooks

```rust
#[async_trait::async_trait]
impl Spider for MySpider {
    // ...

    async fn open(&self) -> Result<(), KumoError> {
        // e.g. open a database connection, create a temp file
        println!("crawl starting");
        Ok(())
    }

    async fn close(&self, stats: &CrawlStats) -> Result<(), KumoError> {
        println!(
            "done: {} pages, {} items, {} errors",
            stats.pages_crawled, stats.items_scraped, stats.errors
        );
        Ok(())
    }
}
```

`CrawlStats` fields:

| Field | Type | Description |
|-------|------|-------------|
| `pages_crawled` | `u64` | Responses processed |
| `items_scraped` | `u64` | Items passed to the store |
| `errors` | `u64` | Failed requests |
| `duration` | `Duration` | Wall-clock crawl time |
| `bytes_downloaded` | `u64` | Total response body bytes |
| `timings` | `CrawlTimingStats` | Cumulative successful-request phase timings for middleware, fetch, parse, pipeline, and store work |
| `interrupted` | `bool` | `true` if stopped by Ctrl+C |
| `error_kinds` | `BTreeMap<String, u64>` | Permanent failures grouped by stable `KumoErrorKind` label |
| `stop_reason` | `Option<StopReason>` | Why the crawl stopped |
| `scheduled` | `u64` | Requests accepted by the scheduler |
| `deduped` | `u64` | Requests skipped because their fingerprint was already seen |
| `retries` | `u64` | Retry attempts requeued by retry policy or `ErrorPolicy::Retry` |
| `retry_exhausted` | `u64` | URLs that permanently failed after retry capacity was exhausted |
| `robots_blocked` | `u64` | Requests skipped because robots.txt disallowed them |
| `domains` | `BTreeMap<String, DomainStats>` | Per-domain counters for scheduled, deduped, completed, failed, error kinds, retries, retry exhaustion, and robots-blocked requests |

`errors` counts permanent request failures, including exhausted retries,
unhandled fetch/parse errors, and crawl task panics. Panics are attributed to
the request's domain in `domains[domain].failed` so production reports do not
silently lose failed work.
Use `retry_exhausted` when alerts need to distinguish "we retried and still
failed" from one-off permanent failures.
Use `CrawlReport::retry_summary()` when alerts need a compact production signal
that separates retry pressure from retry exhaustion.
Use `error_kinds` when alerts or dashboards need to separate parse failures,
HTTP status failures, fetch failures, and other `KumoErrorKind` categories.
Use `timings` to identify the largest successful-request phase. Timing totals
are cumulative across concurrent tasks, so they can be larger than `duration`.

When updating stats manually, use `record_error(domain)` to increment both the
global error count and the matching per-domain failure count together. Use
`record_error_kind(domain, kind)` when you also know the `KumoErrorKind`.

`stop_reason` is set when the crawl ends:

| Reason | Meaning |
|--------|---------|
| `FrontierExhausted` | No scheduled or in-flight requests remain |
| `Interrupted` | The crawl received Ctrl+C or stream cancellation |
| `MaxPages` | `max_pages()` was reached |
| `MaxItems` | `max_items()` was reached after a response finished |
| `MaxDuration` | `max_duration()` was reached |
| `MaxErrors` | `max_errors()` was reached |

Use `CrawlReport::from(stats)` when you need a stable snapshot for logging or
export. Reports can be exported directly with `to_json_value()`,
`to_json_string()`, or `to_json_string_pretty()`:

```rust
let stats = CrawlEngine::builder()
    .run(MySpider)
    .await?;

let report = CrawlReport::from(stats);
std::fs::write("crawl-report.json", report.to_json_string_pretty())?;
```

`CrawlReport` also exposes derived helpers for production dashboards and alerts:

| Helper | Meaning |
|--------|---------|
| `pages_per_second()` | Successful pages divided by crawl duration |
| `items_per_second()` | Scraped items divided by crawl duration |
| `bytes_per_second()` | Downloaded response bytes divided by crawl duration |
| `error_rate()` | Failed requests divided by completed and failed requests |
| `success_rate()` | Completed requests divided by completed and failed requests |
| `retry_exhaustion_rate()` | Retry-exhausted requests divided by retry attempts |
| `retry_summary()` | Retry attempts, exhausted retries, retry pressure, exhaustion, and exhausted-failure rates |

Report JSON uses stable snake_case field names. `duration` is exported as
`duration_ms` and `duration_secs`, derived helper values are exported as fields
such as `pages_per_second` and `error_rate`, timing breakdowns are exported
under `timings`, retry health is exported under `retry_summary`, and
`stop_reason` is exported as a snake_case string such as `"frontier_exhausted"`
or `"max_pages"`.

## Error Handling

`on_error` lets each spider decide what to do with a failed URL:

```rust
fn on_error(&self, url: &str, err: &KumoError) -> ErrorPolicy {
    if matches!(err.kind(), kumo::error::KumoErrorKind::DomainNotAllowed)
        || url.contains("/optional/")
    {
        ErrorPolicy::Skip    // log and continue
    } else {
        ErrorPolicy::Abort   // stop the entire crawl
    }
}
```

Use `err.kind()` when you need stable error classification for metrics,
logging, or custom retry decisions. This avoids matching on display text.

## Domain & Depth Filtering

```rust
fn allowed_domains(&self) -> Vec<&str> {
    vec!["example.com"]  // subdomains are included automatically
}

fn max_depth(&self) -> Option<usize> {
    Some(3)  // don't follow links more than 3 hops from start_urls
}
```

## CrawlEngine Builder

`CrawlEngine::builder()` is a fluent builder that configures and launches the engine:

```rust
CrawlEngine::builder()
    .concurrency(8)                           // max parallel requests (default: 8)
    .crawl_delay(Duration::from_millis(500))  // fixed delay between requests
    .retry(3, Duration::from_millis(200))     // retry up to 3× with 200ms base delay
    .respect_robots_txt(true)                 // honours robots.txt (default: true)
    .max_urls(500_000)                        // Bloom filter size (default: 1_000_000)
    .max_pages(10_000)                        // stop after enough pages
    .max_items(100_000)                       // stop after enough items
    .max_duration(Duration::from_secs(3600))  // stop after elapsed wall-clock time
    .max_errors(100)                          // stop after permanent failures
    .metrics_interval(Duration::from_secs(30))
    .middleware(DefaultHeaders::new().user_agent("my-bot/1.0"))
    .store(JsonlStore::new("output.jsonl")?)
    .run(MySpider)
    .await?;
```

## Multi-Spider Engine

Run multiple independent spiders in one process — each gets its own frontier:

```rust
CrawlEngine::builder()
    .concurrency(4)
    .add_spider(QuotesSpider)
    .add_spider(BooksSpider)
    .run_all()
    .await?;
```

Each spider's `parse()` is called only for URLs in its own frontier. Items from all spiders flow to the same store.

