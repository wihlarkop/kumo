use std::collections::HashSet;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::error::KumoError;

use super::Pipeline;

/// Drops items where a given field's value has already been seen.
///
/// # Example
/// ```rust,ignore
/// .pipeline(DropDuplicates::by_field("url"))
/// ```
pub struct DropDuplicates {
    field: String,
    seen: Mutex<HashSet<String>>,
}

impl DropDuplicates {
    pub fn by_field(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            seen: Mutex::new(HashSet::new()),
        }
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

        let mut seen = self.seen.lock().await;
        if seen.contains(&key) {
            return Ok(None);
        }
        seen.insert(key);
        Ok(Some(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn first_item_passes_through() {
        let p = DropDuplicates::by_field("url");
        let item = json!({"url": "https://example.com"});
        assert!(p.process(item).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn duplicate_is_dropped() {
        let p = DropDuplicates::by_field("url");
        let item = json!({"url": "https://example.com"});
        p.process(item.clone()).await.unwrap();
        assert!(p.process(item).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn different_urls_both_pass() {
        let p = DropDuplicates::by_field("url");
        assert!(
            p.process(json!({"url": "https://a.com"}))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            p.process(json!({"url": "https://b.com"}))
                .await
                .unwrap()
                .is_some()
        );
    }
}
