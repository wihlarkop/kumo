use super::ItemStore;
use crate::error::KumoError;

/// Prints each item as a JSON line to stdout. Useful for piping output.
pub struct StdoutStore;

#[async_trait::async_trait]
impl ItemStore for StdoutStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let json = serde_json::to_string(item).map_err(|e| KumoError::store("stdout serialization", e))?;
        println!("{json}");
        Ok(())
    }
}
