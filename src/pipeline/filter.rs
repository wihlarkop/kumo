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
                tracing::debug!(
                    target: "kumo::item",
                    missing_field = %field,
                    reason = "missing_field",
                    "item.drop"
                );
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
            tracing::debug!(
                target: "kumo::item",
                reason = "filter",
                "item.drop"
            );
            Ok(None)
        }
    }
}
