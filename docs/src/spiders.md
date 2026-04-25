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

`parse()` returns `Output<T>` — a builder that collects items and URLs to follow:

```rust
Output::new()
    .item(my_item)               // add one item
    .items(vec![a, b, c])        // add many items
    .follow("https://next-page") // enqueue a URL
    .follow_many(links)          // enqueue many URLs
```

Items are serialized to JSON exactly once and passed to pipelines and the store.

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
| `interrupted` | `bool` | `true` if stopped by Ctrl+C |

## Error Handling

`on_error` lets each spider decide what to do with a failed URL:

```rust
fn on_error(&self, url: &str, err: &KumoError) -> ErrorPolicy {
    if url.contains("/optional/") {
        ErrorPolicy::Skip    // log and continue
    } else {
        ErrorPolicy::Abort   // stop the entire crawl
    }
}
```

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
