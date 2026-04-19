use super::ItemStore;
use crate::error::KumoError;
use std::{path::PathBuf, sync::Mutex};

/// Accumulates all scraped items in memory and writes a pretty-printed
/// JSON array to disk on `flush()` (called automatically by the engine).
///
/// Best for small-to-medium crawls where you want human-readable output.
/// For streaming/large crawls, prefer `JsonlStore`.
pub struct JsonStore {
    path: PathBuf,
    items: Mutex<Vec<serde_json::Value>>,
}

impl JsonStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("failed to create directory: {}", e));
        }
        Self {
            path,
            items: Mutex::new(Vec::new()),
        }
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
