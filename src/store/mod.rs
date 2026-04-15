pub mod jsonl;
pub mod stdout;

pub use jsonl::JsonlStore;
pub use stdout::StdoutStore;

use crate::error::KumoError;

/// Pluggable output backend for scraped items.
///
/// Items arrive pre-serialized as `serde_json::Value` from `Output::item()`.
/// Implement this trait to send scraped data to a database, Kafka, S3, etc.
#[async_trait::async_trait]
pub trait ItemStore: Send + Sync {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError>;

    /// Flush any buffered writes. Called by the engine after the crawl finishes.
    async fn flush(&self) -> Result<(), KumoError> {
        Ok(())
    }
}
