use super::ItemStore;
use crate::error::KumoError;

/// Prints each item as a JSON line to stdout. Useful for piping output.
#[derive(Debug)]
pub struct StdoutStore;

#[async_trait::async_trait]
impl ItemStore for StdoutStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let json =
            serde_json::to_string(item).map_err(|e| KumoError::store("stdout serialization", e))?;
        println!("{json}");
        Ok(())
    }

    async fn store_many(&self, items: &[serde_json::Value]) -> Result<(), KumoError> {
        for item in items {
            let json = serde_json::to_string(item)
                .map_err(|e| KumoError::store("stdout serialization", e))?;
            println!("{json}");
        }
        Ok(())
    }
}
