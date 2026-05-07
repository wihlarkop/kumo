use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::error::KumoError;

use super::Pipeline;

struct DropDuplicatesInner {
    seen: HashSet<String>,
    writer: Option<BufWriter<File>>,
}

/// Drops items where a given field's value has already been seen.
///
/// # Examples
///
/// In-memory (resets on restart):
/// ```rust,ignore
/// .pipeline(DropDuplicates::by_field("url"))
/// ```
///
/// Persistent (survives Ctrl+C / restarts):
/// ```rust,ignore
/// .pipeline(DropDuplicates::with_persistence("url", "seen.txt"))
/// ```
pub struct DropDuplicates {
    field: String,
    inner: Mutex<DropDuplicatesInner>,
}

impl DropDuplicates {
    /// Create an in-memory deduplicator. Seen keys are lost on restart.
    pub fn by_field(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            inner: Mutex::new(DropDuplicatesInner {
                seen: HashSet::new(),
                writer: None,
            }),
        }
    }

    /// Create a persistent deduplicator backed by `path`.
    ///
    /// On startup, all keys previously written to `path` are loaded as already-seen.
    /// Each new key is appended to the file as it is accepted, so the state survives
    /// restarts and graceful-shutdown interruptions.
    pub fn with_persistence(
        field: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<Self, crate::error::KumoError> {
        let path = path.into();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::KumoError::store("dedup persistence", e))?;
        }

        // Load previously-seen keys.
        let seen: HashSet<String> = if path.exists() {
            let file = File::open(&path)
                .map_err(|e| crate::error::KumoError::store("dedup persistence", e))?;
            BufReader::new(file)
                .lines()
                .map_while(|l| l.ok())
                .filter(|l| !l.is_empty())
                .collect()
        } else {
            HashSet::new()
        };

        // Open for append-only writes.
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| crate::error::KumoError::store("dedup persistence", e))?;

        Ok(Self {
            field: field.into(),
            inner: Mutex::new(DropDuplicatesInner {
                seen,
                writer: Some(BufWriter::new(file)),
            }),
        })
    }
}

#[async_trait]
impl Pipeline for DropDuplicates {
    async fn process(
        &self,
        item: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, KumoError> {
        let key = item
            .get(&self.field)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| item.to_string());

        let mut inner = self.inner.lock().await;
        if inner.seen.contains(&key) {
            tracing::debug!(field = %self.field, key = %key, "item.drop.duplicate");
            return Ok(None);
        }

        if let Some(ref mut writer) = inner.writer {
            writeln!(writer, "{key}").map_err(|e| KumoError::store("dedup persistence", e))?;
        }

        inner.seen.insert(key);
        Ok(Some(item))
    }
}
