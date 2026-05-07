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
