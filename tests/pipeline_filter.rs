use kumo::pipeline::{FilterPipeline, Pipeline, RequireFields};
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
    let item = json!({"title": "Foo"});
    assert!(p.process(item).await.unwrap().is_none());
}

#[tokio::test]
async fn filter_pipeline_passes_matching_item() {
    let p = FilterPipeline::new(|item: &serde_json::Value| item["value"].as_i64().unwrap_or(0) > 0);
    assert!(p.process(json!({"value": 5})).await.unwrap().is_some());
}

#[tokio::test]
async fn filter_pipeline_drops_non_matching_item() {
    let p = FilterPipeline::new(|item: &serde_json::Value| item["value"].as_i64().unwrap_or(0) > 0);
    assert!(p.process(json!({"value": -1})).await.unwrap().is_none());
}
