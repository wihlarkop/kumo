# Stores

Stores persist scraped items. Set one with `.store()` on the engine builder. All stores implement the `ItemStore` trait.

## File Stores (default, no feature flag)

### JsonlStore

One JSON object per line — most efficient for large crawls:

```rust
.store(JsonlStore::new("output.jsonl")?)
```

### JsonStore

Pretty-printed JSON array written atomically on crawl completion:

```rust
.store(JsonStore::new("output.json")?)
```

### CsvStore

CSV with headers derived from the item's field names:

```rust
.store(CsvStore::new("output.csv")?)
```

### StdoutStore

Prints each item as JSON to stdout — useful for debugging:

```rust
.store(StdoutStore)
// or just omit .store() — StdoutStore is the default
```

## Store Buffering and Backpressure

By default, request tasks write accepted items directly to the configured store.
This preserves simple ordering and reports direct store time under
`CrawlReport::timings.store`.

For high-concurrency crawls or slow downstream stores, enable Kumo's bounded
store buffer:

```rust
CrawlEngine::builder()
    .concurrency(32)
    .store(JsonlStore::new("items.jsonl")?)
    .store_buffer(1_000, 100) // queue capacity, batch size
    .run(MySpider)
    .await?;
```

Accepted items are queued into a background writer. If the queue is full,
request tasks wait for capacity, so store pressure is bounded instead of
growing memory without limit. The writer passes up to `batch_size` items to
`ItemStore::store_many()`.

Custom stores do not need to change. `store_many()` defaults to calling
`store()` for each item, while stores that support efficient batch writes can
override it.

Final reports include `report.store`:

| Field | Meaning |
|---|---|
| `buffered` | Whether the bounded store buffer was enabled |
| `queue_capacity` | Configured maximum queued items |
| `batch_size` | Configured maximum write batch size |
| `queued` | Items accepted into the queue |
| `written` | Items written by the background writer |
| `batches` | Non-empty `store_many()` calls |
| `queue_full_waits` | Times an item observed a full queue before waiting |
| `queue_wait` | Total time request tasks spent waiting to enqueue items |
| `write` | Total time the writer spent inside store writes |

For `run_all()`, the store is shared by all registered spiders, so each returned
`CrawlStats` receives the same aggregate store-buffer counters.

## PostgreSQL

Requires `features = ["postgres"]`.

```toml
kumo = { version = "0.2", features = ["postgres"] }
```

```rust
let store = PostgresStore::builder("postgres://user:pass@localhost/mydb")
    .table("items")
    .connect()
    .await?;

.store(store)
```

The store creates the table if it does not exist. Each item is inserted as a JSONB row.

## SQLite

Requires `features = ["sqlite"]`.

```toml
kumo = { version = "0.2", features = ["sqlite"] }
```

```rust
let store = SqliteStore::builder("sqlite://crawl.db")
    .table("items")
    .connect()
    .await?;
.store(store)
```

## MySQL

Requires `features = ["mysql"]`.

```toml
kumo = { version = "0.2", features = ["mysql"] }
```

```rust
let store = MySqlStore::builder("mysql://user:pass@localhost/mydb")
    .table("items")
    .connect()
    .await?;

.store(store)
```

## Cloud Storage

Requires `features = ["cloud"]`. For specific providers, also enable `cloud-s3`, `cloud-gcs`, or `cloud-azure`.

`CloudStore` wraps any [`object_store`](https://docs.rs/object_store) backend — S3, GCS, Azure Blob, local filesystem, or in-memory. You configure the backend yourself and pass it in as `Arc<dyn ObjectStore>`, so kumo has no hardcoded cloud logic.

Items are buffered in memory during the crawl and written as a single object on completion.

### Supported backends

| Backend | Feature flag | `object_store` type | Credentials |
|---|---|---|---|
| Amazon S3 | `cloud-s3` | `AmazonS3Builder` | env / IAM / explicit |
| Google Cloud Storage | `cloud-gcs` | `GoogleCloudStorageBuilder` | Application Default / service account |
| Azure Blob Storage | `cloud-azure` | `MicrosoftAzureBuilder` | connection string / managed identity |
| Local filesystem | `cloud` | `LocalFileSystem` | none |
| In-memory (testing) | `cloud` | `InMemory` | none |

```toml
# base — enables LocalFileSystem + InMemory backends
kumo = { version = "0.2", features = ["cloud"] }

# S3
kumo = { version = "0.2", features = ["cloud-s3"] }

# GCS
kumo = { version = "0.2", features = ["cloud-gcs"] }

# Azure Blob
kumo = { version = "0.2", features = ["cloud-azure"] }
```

### Local filesystem (dev / CI)

```rust
use std::sync::Arc;
use object_store::local::LocalFileSystem;
use kumo::store::CloudStore;

let backend = Arc::new(LocalFileSystem::new_with_prefix("/tmp/crawl")?);
let store = CloudStore::builder(backend)
    .prefix("quotes")          // object path prefix
    .build();                  // filename: items-<millis>.jsonl

.store(store)
```

### Amazon S3 (`cloud-s3`)

```rust
use std::sync::Arc;
use object_store::aws::AmazonS3Builder;
use kumo::store::CloudStore;

let s3 = Arc::new(
    AmazonS3Builder::new()
        .with_bucket_name("my-bucket")
        .with_region("us-east-1")
        .build()?,
);

let store = CloudStore::builder(s3)
    .prefix("crawls/2024")
    .build();
```

### Google Cloud Storage (`cloud-gcs`)

```rust
use std::sync::Arc;
use object_store::gcp::GoogleCloudStorageBuilder;
use kumo::store::CloudStore;

let gcs = Arc::new(
    GoogleCloudStorageBuilder::new()
        .with_bucket_name("my-bucket")
        .build()?,
);

let store = CloudStore::builder(gcs).prefix("crawls").build();
```

### Azure Blob Storage (`cloud-azure`)

```rust
use std::sync::Arc;
use object_store::azure::MicrosoftAzureBuilder;
use kumo::store::CloudStore;

let azure = Arc::new(
    MicrosoftAzureBuilder::new()
        .with_container_name("my-container")
        .with_account("my-account")
        .build()?,
);

let store = CloudStore::builder(azure).prefix("crawls").build();
```

### Output format

JSONL is the default (one JSON object per line — compatible with BigQuery, Redshift, Spark). Switch to a JSON array with `.format(CloudFormat::Json)`:

```rust
use kumo::store::{CloudStore, CloudFormat};

let store = CloudStore::builder(backend)
    .format(CloudFormat::Json)
    .filename("results.json")
    .build();
```

### Custom filename

By default the filename is `items-<unix_millis>.jsonl` (or `.json`). Override it:

```rust
CloudStore::builder(backend)
    .prefix("daily/2024-04-27")
    .filename("quotes.jsonl")
    .build();
```

## Custom Store

Implement `ItemStore` to write to any destination:

```rust
use kumo::prelude::*;
use async_trait::async_trait;

pub struct KafkaStore { /* ... */ }

#[async_trait]
impl ItemStore for KafkaStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        // publish item to Kafka topic
        Ok(())
    }
}
```

