# Pipelines

Pipelines transform or filter items before they reach the store. Register them with `.pipeline()` — they run in order before the store receives each item.

## DropDuplicates

Drops items with duplicate values for a given field:

```rust
.pipeline(DropDuplicates::by_field("url"))
// or on multiple fields
.pipeline(DropDuplicates::on("title"))
```

Uses an in-memory `HashSet`. For large crawls combine with `max_urls` to right-size the Bloom filter.

## FilterPipeline

Keep only items matching a predicate:

```rust
.pipeline(
    FilterPipeline::new(|item: &serde_json::Value| {
        item["price"].as_f64().map(|p| p > 0.0).unwrap_or(false)
    })
)
```

Items where the predicate returns `false` are dropped silently.

## RequireFields

Drop items that are missing required fields (null or missing key):

```rust
.pipeline(RequireFields::new(&["title", "url", "price"]))
```

Useful for catching partial extractions before they pollute the store.

## Custom Pipeline

```rust
use kumo::prelude::*;
use async_trait::async_trait;

pub struct NormalizePrice;

#[async_trait]
impl Pipeline for NormalizePrice {
    async fn process(
        &self,
        mut item: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, KumoError> {
        if let Some(price) = item["price"].as_str() {
            let cleaned = price.replace(['$', ','], "");
            item["price"] = serde_json::json!(cleaned);
        }
        Ok(Some(item))   // return None to drop the item
    }
}
```

Return `Ok(None)` to drop the item, `Ok(Some(item))` to pass it through (possibly modified).

## Chaining Example

```rust
CrawlEngine::builder()
    .pipeline(RequireFields::new(&["title", "price"]))
    .pipeline(DropDuplicates::by_field("url"))
    .pipeline(NormalizePrice)
    .store(JsonlStore::new("products.jsonl")?)
    .run(ProductSpider)
    .await?;
```

Pipelines run in registration order — items pass through `RequireFields` → `DropDuplicates` → `NormalizePrice` → store.
