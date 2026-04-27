use super::ItemStore;
use crate::error::KumoError;
use std::{path::PathBuf, sync::Mutex};

/// Accumulates all scraped items in memory and writes a pretty-printed
/// JSON array to disk on `flush()` (called automatically by the engine).
///
/// Best for small-to-medium crawls where you want human-readable output.
/// For streaming/large crawls, prefer `JsonlStore`.
#[derive(Debug)]
pub struct JsonStore {
    path: PathBuf,
    items: Mutex<Vec<serde_json::Value>>,
}

impl JsonStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, KumoError> {
        let path = path.into();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| KumoError::store("json store", e))?;
        }
        Ok(Self {
            path,
            items: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait::async_trait]
impl ItemStore for JsonStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        self.items.lock().unwrap().push(item.clone());
        Ok(())
    }

    async fn flush(&self) -> Result<(), KumoError> {
        let items = self.items.lock().unwrap();
        let json =
            serde_json::to_string_pretty(&*items).map_err(|e| KumoError::store("json store", e))?;
        std::fs::write(&self.path, json).map_err(|e| KumoError::store("json store", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_store_is_debug() {
        let store = JsonStore::new("test_debug.json").unwrap();
        let s = format!("{store:?}");
        assert!(s.contains("JsonStore"), "got: {s}");
        let _ = std::fs::remove_file("test_debug.json");
    }
}
