use super::ItemStore;
use crate::error::KumoError;
use std::{
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};

/// Appends one JSON object per line to a file (JSON Lines format).
///
/// Creates the file and all parent directories on construction.
/// Uses a `std::sync::Mutex`-guarded `BufWriter` for thread-safe writes.
#[derive(Debug)]
pub struct JsonlStore {
    writer: Mutex<BufWriter<std::fs::File>>,
}

impl JsonlStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, KumoError> {
        let path = path.into();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| KumoError::store("jsonl store", e))?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| KumoError::store("jsonl store", e))?;
        Ok(Self {
            writer: Mutex::new(BufWriter::new(file)),
        })
    }
}

#[async_trait::async_trait]
impl ItemStore for JsonlStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let json = serde_json::to_string(item).map_err(|e| KumoError::store("jsonl store", e))?;
        let mut writer = self.writer.lock().unwrap();
        writeln!(writer, "{json}").map_err(|e| KumoError::store("jsonl store", e))?;
        Ok(())
    }

    async fn flush(&self) -> Result<(), KumoError> {
        self.writer
            .lock()
            .unwrap()
            .flush()
            .map_err(|e| KumoError::store("jsonl store", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_store_is_debug() {
        let store = JsonlStore::new("test_debug.jsonl").unwrap();
        let s = format!("{store:?}");
        assert!(s.contains("JsonlStore"), "got: {s}");
        let _ = std::fs::remove_file("test_debug.jsonl");
    }
}
