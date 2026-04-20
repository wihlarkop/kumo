pub mod dedup;
pub mod filter;

pub use dedup::DropDuplicates;
pub use filter::{FilterPipeline, RequireFields};

use crate::error::KumoError;

/// Processing stage applied to every item before it reaches the `ItemStore`.
///
/// Pipelines are applied in registration order. Each stage receives the item
/// produced by the previous stage (or the raw scraped item for the first stage).
///
/// # Return values
///
/// - `Ok(Some(item))` — pass the item to the next stage / store.
/// - `Ok(None)` — **drop** the item silently (no error, no store write).
/// - `Err(e)` — log the error and drop the item; the crawl continues.
///
/// # Example
///
/// ```rust,ignore
/// use kumo::prelude::*;
/// use async_trait::async_trait;
///
/// struct PriceFilter;
///
/// #[async_trait]
/// impl Pipeline for PriceFilter {
///     async fn process(
///         &self,
///         item: serde_json::Value,
///     ) -> Result<Option<serde_json::Value>, KumoError> {
///         // Drop items where price is missing or zero.
///         let price = item["price"].as_f64().unwrap_or(0.0);
///         if price <= 0.0 { return Ok(None); }
///         Ok(Some(item))
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait Pipeline: Send + Sync {
    async fn process(
        &self,
        item: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, KumoError>;
}
