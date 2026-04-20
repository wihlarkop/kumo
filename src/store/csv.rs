use super::ItemStore;
use crate::error::KumoError;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};

pub struct CsvStore {
    path: std::path::PathBuf,
    preset_headers: Option<Vec<String>>,
    inner: Mutex<CsvInner>,
}

impl std::fmt::Debug for CsvStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsvStore")
            .field("path", &self.path)
            .field("preset_headers", &self.preset_headers)
            .finish()
    }
}

struct CsvInner {
    writer: BufWriter<File>,
    key_order: Option<Vec<String>>,
}

/// RFC 4180: quote any field that contains a comma, double-quote, CR, or LF.
/// Interior double-quotes are escaped by doubling them.
fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

impl CsvStore {
    /// Open (or create) a CSV file. Headers are derived from the keys of the
    /// first item stored and written as the first line.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, KumoError> {
        let path = path.into();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| KumoError::store("csv store", e))?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| KumoError::store("csv store", e))?;
        Ok(Self {
            path,
            preset_headers: None,
            inner: Mutex::new(CsvInner {
                writer: BufWriter::new(file),
                key_order: None,
            }),
        })
    }

    /// Open (or create) a CSV file with an explicit column order.
    /// Columns not present in an item are written as empty cells.
    pub fn with_headers(path: impl Into<PathBuf>, headers: &[&str]) -> Result<Self, KumoError> {
        let mut store = Self::new(path)?;
        store.preset_headers = Some(headers.iter().map(|s| s.to_string()).collect());
        Ok(store)
    }
}

#[async_trait::async_trait]
impl ItemStore for CsvStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let obj = item
            .as_object()
            .ok_or_else(|| KumoError::store_msg("csv store: item must be a JSON object"))?;

        let mut inner = self.inner.lock().unwrap();

        if inner.key_order.is_none() {
            let keys: Vec<String> = if let Some(ref preset) = self.preset_headers {
                preset.clone()
            } else {
                obj.keys().cloned().collect()
            };
            let header_line = keys
                .iter()
                .map(|k| csv_escape(k))
                .collect::<Vec<_>>()
                .join(",");
            writeln!(inner.writer, "{header_line}")
                .map_err(|e| KumoError::store("csv store", e))?;
            inner.key_order = Some(keys);
        }

        let keys = inner.key_order.as_ref().unwrap();
        let row: Vec<String> = keys
            .iter()
            .map(|k| {
                obj.get(k)
                    .map(|v| match v {
                        serde_json::Value::String(s) => csv_escape(s),
                        serde_json::Value::Null => String::new(),
                        other => csv_escape(&other.to_string()),
                    })
                    .unwrap_or_default()
            })
            .collect();
        writeln!(inner.writer, "{}", row.join(","))
            .map_err(|e| KumoError::store("csv store", e))?;

        Ok(())
    }

    async fn flush(&self) -> Result<(), KumoError> {
        self.inner
            .lock()
            .unwrap()
            .writer
            .flush()
            .map_err(|e| KumoError::store("csv store", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::NamedTempFile;

    fn make_store_at(path: &std::path::Path) -> CsvStore {
        CsvStore::new(path).unwrap()
    }

    #[tokio::test]
    async fn auto_headers_and_rows() {
        let tmp = NamedTempFile::new().unwrap();
        let store = make_store_at(tmp.path());

        store
            .store(&json!({"title": "Hello", "url": "https://example.com"}))
            .await
            .unwrap();
        store
            .store(&json!({"title": "World", "url": "https://example.org"}))
            .await
            .unwrap();
        store.flush().await.unwrap();

        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines[0], "title,url");
        assert_eq!(lines[1], "Hello,https://example.com");
        assert_eq!(lines[2], "World,https://example.org");
    }

    #[tokio::test]
    async fn commas_are_quoted() {
        let tmp = NamedTempFile::new().unwrap();
        let store = make_store_at(tmp.path());

        store
            .store(&json!({"value": "one, two, three"}))
            .await
            .unwrap();
        store.flush().await.unwrap();

        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines[0], "value");
        assert_eq!(lines[1], "\"one, two, three\"");
    }

    #[tokio::test]
    async fn interior_quotes_are_doubled() {
        let tmp = NamedTempFile::new().unwrap();
        let store = make_store_at(tmp.path());

        store
            .store(&json!({"value": r#"say "hello""#}))
            .await
            .unwrap();
        store.flush().await.unwrap();

        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        // RFC 4180: "say ""hello""" — opening ", doubled interior quotes, closing "
        assert_eq!(lines[1], "\"say \"\"hello\"\"\"");
    }

    #[tokio::test]
    async fn with_headers_sets_column_order() {
        let tmp = NamedTempFile::new().unwrap();
        let store = CsvStore::with_headers(tmp.path(), &["url", "title"]).unwrap();

        store
            .store(&json!({"title": "Hello", "url": "https://example.com"}))
            .await
            .unwrap();
        store.flush().await.unwrap();

        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines[0], "url,title");
        assert_eq!(lines[1], "https://example.com,Hello");
    }

    #[tokio::test]
    async fn missing_key_becomes_empty_cell() {
        let tmp = NamedTempFile::new().unwrap();
        let store = CsvStore::with_headers(tmp.path(), &["title", "price"]).unwrap();

        store.store(&json!({"title": "Widget"})).await.unwrap();
        store.flush().await.unwrap();

        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines[1], "Widget,");
    }
}
