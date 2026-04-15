use std::{
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};
use crate::error::KumoError;
use super::ItemStore;

/// Appends one JSON object per line to a file (JSON Lines format).
///
/// Creates the file and all parent directories on construction.
/// Uses a `std::sync::Mutex`-guarded `BufWriter` for thread-safe writes.
pub struct JsonlStore {
    writer: Mutex<BufWriter<std::fs::File>>,
}

impl JsonlStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .unwrap_or_else(|e| panic!("failed to create directory {}: {}", parent.display(), e));
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|e| panic!("failed to open {}: {}", path.display(), e));
        Self {
            writer: Mutex::new(BufWriter::new(file)),
        }
    }
}

#[async_trait::async_trait]
impl ItemStore for JsonlStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let json = serde_json::to_string(item)
            .map_err(|e| KumoError::Store(e.to_string()))?;
        let mut writer = self.writer.lock().unwrap();
        writeln!(writer, "{json}").map_err(|e| KumoError::Store(e.to_string()))?;
        Ok(())
    }

    async fn flush(&self) -> Result<(), KumoError> {
        self.writer
            .lock()
            .unwrap()
            .flush()
            .map_err(|e| KumoError::Store(e.to_string()))
    }
}
