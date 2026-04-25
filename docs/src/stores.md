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

## PostgreSQL

Requires `features = ["postgres"]`.

```toml
kumo = { version = "0.1", features = ["postgres"] }
```

```rust
let store = PostgresStore::new(
    "postgres://user:pass@localhost/mydb",
    "items",  // table name
).await?;

.store(store)
```

The store creates the table if it does not exist. Each item is inserted as a JSONB row.

## SQLite

Requires `features = ["sqlite"]`.

```toml
kumo = { version = "0.1", features = ["sqlite"] }
```

```rust
let store = SqliteStore::new("sqlite://crawl.db", "items").await?;
.store(store)
```

## MySQL

Requires `features = ["mysql"]`.

```toml
kumo = { version = "0.1", features = ["mysql"] }
```

```rust
let store = MySqlStore::new(
    "mysql://user:pass@localhost/mydb",
    "items",
).await?;

.store(store)
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
