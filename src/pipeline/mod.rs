pub mod dedup;
pub mod filter;

pub use dedup::DropDuplicates;
pub use filter::{FilterPipeline, RequireFields};

use crate::error::KumoError;

/// Processing stage applied to every item before it reaches the `ItemStore`.
///
/// Return `Ok(Some(item))` to pass the item downstream, `Ok(None)` to drop it,
/// or `Err` to fail the item (logged but not fatal — the crawl continues).
#[async_trait::async_trait]
pub trait Pipeline: Send + Sync {
    async fn process(
        &self,
        item: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, KumoError>;
}
