use async_trait::async_trait;

use crate::error::KumoError;

use super::Pipeline;

/// Drops items that are missing any of the required fields.
///
/// # Example
/// ```rust,ignore
/// .pipeline(RequireFields::new(&["title", "price"]))
/// ```
pub struct RequireFields {
    fields: Vec<String>,
}

impl RequireFields {
    pub fn new(fields: &[&str]) -> Self {
        Self {
            fields: fields.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[async_trait]
impl Pipeline for RequireFields {
    async fn process(
        &self,
        item: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, KumoError> {
        for field in &self.fields {
            if item.get(field).is_none() {
                tracing::debug!(missing_field = %field, "item.drop.missing_field");
                return Ok(None);
            }
        }
        Ok(Some(item))
    }
}

/// Drops items that do not satisfy a synchronous predicate.
///
/// # Example
/// ```rust,ignore
/// .pipeline(FilterPipeline::new(|item| {
///     item["price"].as_f64().map(|p| p > 0.0).unwrap_or(false)
/// }))
/// ```
pub struct FilterPipeline<F>
where
    F: Fn(&serde_json::Value) -> bool + Send + Sync,
{
    predicate: F,
}

impl<F> FilterPipeline<F>
where
    F: Fn(&serde_json::Value) -> bool + Send + Sync,
{
    pub fn new(predicate: F) -> Self {
        Self { predicate }
    }
}

#[async_trait]
impl<F> Pipeline for FilterPipeline<F>
where
    F: Fn(&serde_json::Value) -> bool + Send + Sync,
{
    async fn process(
        &self,
        item: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, KumoError> {
        if (self.predicate)(&item) {
            Ok(Some(item))
        } else {
            tracing::debug!("item.drop.filter");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn require_fields_passes_complete_item() {
        let p = RequireFields::new(&["title", "url"]);
        let item = json!({"title": "Foo", "url": "https://example.com"});
        assert!(p.process(item).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn require_fields_drops_incomplete_item() {
        let p = RequireFields::new(&["title", "price"]);
        let item = json!({"title": "Foo"}); // missing price
        assert!(p.process(item).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn filter_pipeline_passes_matching_item() {
        let p = FilterPipeline::new(|item| item["value"].as_i64().unwrap_or(0) > 0);
        assert!(p.process(json!({"value": 5})).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn filter_pipeline_drops_non_matching_item() {
        let p = FilterPipeline::new(|item| item["value"].as_i64().unwrap_or(0) > 0);
        assert!(p.process(json!({"value": -1})).await.unwrap().is_none());
    }
}
