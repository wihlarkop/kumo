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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdout_store_is_debug() {
        let s = format!("{:?}", StdoutStore);
        assert!(s.contains("StdoutStore"), "got: {s}");
    }
}
