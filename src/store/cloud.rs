use std::sync::Arc;

use async_trait::async_trait;
use object_store::path::Path as StorePath;

use super::ItemStore;
use crate::error::KumoError;

#[derive(Debug)]
pub enum CloudFormat {
    Jsonl,
    Json,
}

pub struct CloudStore {
    store: Arc<dyn object_store::ObjectStore>,
    path: StorePath,
    format: CloudFormat,
    items: tokio::sync::Mutex<Vec<serde_json::Value>>,
}

pub struct CloudStoreBuilder {
    store: Arc<dyn object_store::ObjectStore>,
    prefix: String,
    format: CloudFormat,
    filename: Option<String>,
}

impl CloudStore {
    pub fn builder(store: Arc<dyn object_store::ObjectStore>) -> CloudStoreBuilder {
        CloudStoreBuilder {
            store,
            prefix: String::new(),
            format: CloudFormat::Jsonl,
            filename: None,
        }
    }
}

impl CloudStoreBuilder {
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn format(mut self, format: CloudFormat) -> Self {
        self.format = format;
        self
    }

    pub fn filename(mut self, name: impl Into<String>) -> Self {
        self.filename = Some(name.into());
        self
    }

    pub fn build(self) -> CloudStore {
        let filename = self.filename.unwrap_or_else(|| {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let ext = match &self.format {
                CloudFormat::Jsonl => "jsonl",
                CloudFormat::Json => "json",
            };
            format!("items-{ts}.{ext}")
        });

        let path_str = if self.prefix.is_empty() {
            filename
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), filename)
        };

        CloudStore {
            store: self.store,
            path: StorePath::from(path_str),
            format: self.format,
            items: tokio::sync::Mutex::new(Vec::new()),
        }
    }
}

impl std::fmt::Debug for CloudStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudStore")
            .field("path", &self.path)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for CloudStoreBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudStoreBuilder")
            .field("prefix", &self.prefix)
            .field("format", &self.format)
            .field("filename", &self.filename)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ItemStore for CloudStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        self.items.lock().await.push(item.clone());
        Ok(())
    }

    async fn flush(&self) -> Result<(), KumoError> {
        let items = self.items.lock().await;
        if items.is_empty() {
            return Ok(());
        }

        let content = match self.format {
            CloudFormat::Jsonl => {
                let mut buf = String::new();
                for item in items.iter() {
                    let line = serde_json::to_string(item)
                        .map_err(|e| KumoError::store("serialize item to JSONL", e))?;
                    buf.push_str(&line);
                    buf.push('\n');
                }
                buf
            }
            CloudFormat::Json => serde_json::to_string_pretty(&*items)
                .map_err(|e| KumoError::store("serialize items to JSON", e))?,
        };

        let bytes = bytes::Bytes::from(content);
        self.store
            .put(&self.path, bytes.into())
            .await
            .map_err(|e| KumoError::store(format!("upload to {}", self.path), e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use object_store::ObjectStore;
    use object_store::memory::InMemory;
    use object_store::path::Path as StorePath;
    use serde_json::json;

    use super::*;

    fn mem_store() -> Arc<InMemory> {
        Arc::new(InMemory::new())
    }

    #[test]
    fn cloud_format_is_debug() {
        assert_eq!(format!("{:?}", CloudFormat::Jsonl), "Jsonl");
        assert_eq!(format!("{:?}", CloudFormat::Json), "Json");
    }

    #[test]
    fn cloud_store_is_debug() {
        let s = CloudStore::builder(mem_store()).filename("x.jsonl").build();
        let dbg = format!("{s:?}");
        assert!(dbg.contains("CloudStore"), "got: {dbg}");
    }

    #[test]
    fn cloud_store_builder_is_debug() {
        let b = CloudStore::builder(mem_store())
            .prefix("p")
            .filename("f.jsonl");
        let dbg = format!("{b:?}");
        assert!(dbg.contains("CloudStoreBuilder"), "got: {dbg}");
    }

    #[test]
    fn builder_no_prefix_uses_bare_filename() {
        let s = CloudStore::builder(mem_store())
            .filename("items.jsonl")
            .build();
        assert_eq!(s.path, StorePath::from("items.jsonl"));
    }

    #[test]
    fn builder_prefix_is_prepended_with_slash() {
        let s = CloudStore::builder(mem_store())
            .prefix("2024/crawls")
            .filename("items.jsonl")
            .build();
        assert_eq!(s.path, StorePath::from("2024/crawls/items.jsonl"));
    }

    #[test]
    fn builder_trailing_slash_in_prefix_is_normalised() {
        let s = CloudStore::builder(mem_store())
            .prefix("results/")
            .filename("out.jsonl")
            .build();
        assert_eq!(s.path, StorePath::from("results/out.jsonl"));
    }

    #[tokio::test]
    async fn store_buffers_items_in_memory() {
        let s = CloudStore::builder(mem_store())
            .filename("test.jsonl")
            .build();

        s.store(&json!({"a": 1})).await.unwrap();
        s.store(&json!({"a": 2})).await.unwrap();

        let items = s.items.lock().await;
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn jsonl_flush_writes_one_line_per_item() {
        let mem = mem_store();
        let s = CloudStore::builder(mem.clone())
            .prefix("results")
            .filename("test.jsonl")
            .build();

        s.store(&json!({"title": "A"})).await.unwrap();
        s.store(&json!({"title": "B"})).await.unwrap();
        s.flush().await.unwrap();

        let path = StorePath::from("results/test.jsonl");
        let bytes = mem.get(&path).await.unwrap().bytes().await.unwrap();
        let content = std::str::from_utf8(&bytes).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["title"], "A");
        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["title"], "B");
    }

    #[tokio::test]
    async fn json_flush_writes_pretty_array() {
        let mem = mem_store();
        let s = CloudStore::builder(mem.clone())
            .format(CloudFormat::Json)
            .filename("test.json")
            .build();

        s.store(&json!({"n": 1})).await.unwrap();
        s.store(&json!({"n": 2})).await.unwrap();
        s.flush().await.unwrap();

        let path = StorePath::from("test.json");
        let bytes = mem.get(&path).await.unwrap().bytes().await.unwrap();
        let items: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["n"], 1);
        assert_eq!(items[1]["n"], 2);
    }

    #[tokio::test]
    async fn flush_with_no_items_does_not_create_object() {
        let mem = mem_store();
        let s = CloudStore::builder(mem.clone())
            .filename("empty.jsonl")
            .build();

        s.flush().await.unwrap();

        let path = StorePath::from("empty.jsonl");
        assert!(
            mem.get(&path).await.is_err(),
            "flush with no items must not create an object"
        );
    }
}
