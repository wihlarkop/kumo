# URL Frontiers

The frontier is the queue of URLs to crawl. kumo deduplicates URLs with a Bloom filter (O(1), 1M URLs at 0.1% false-positive rate by default).

## MemoryFrontier (default)

The default — no configuration needed. Held in RAM; lost when the process exits.

```rust
CrawlEngine::builder()
    // no .frontier() call — uses MemoryFrontier automatically
    .run(MySpider)
    .await?;
```

## FileFrontier

Requires `features = ["persistence"]`. Persists the URL queue to disk — survives restarts.

```toml
kumo = { version = "0.1", features = ["persistence"] }
```

```rust
use kumo::FileFrontier;

CrawlEngine::builder()
    .frontier(FileFrontier::new("frontier.bin")?)
    .run(MySpider)
    .await?;
```

If `frontier.bin` exists when the process starts, crawling resumes from where it left off. Delete the file to start fresh.

## RedisFrontier

Requires `features = ["redis-frontier"]`. Distributes the URL queue across multiple processes via Redis.

```toml
kumo = { version = "0.1", features = ["redis-frontier"] }
```

```rust
use kumo::RedisFrontier;

let frontier = RedisFrontier::new("redis://127.0.0.1:6379", "my-crawl").await?;

CrawlEngine::builder()
    .frontier(frontier)
    .run(MySpider)
    .await?;
```

Multiple processes can use the same Redis key — they share the queue and deduplication set. Use this for distributed crawls where a single process can't saturate the target site's bandwidth.

## Tuning the Bloom Filter

For crawls smaller than the default 1M URL estimate, shrink the Bloom filter to save RAM:

```rust
CrawlEngine::builder()
    .max_urls(100_000)   // right-size for your crawl
    .run(MySpider)
    .await?;
```

Setting `max_urls` too low increases the false-positive rate (some new URLs skipped as duplicates). Setting it too high wastes memory. Rule of thumb: set it to 2× your expected unique URL count.
