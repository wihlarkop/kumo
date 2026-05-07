use kumo::store::{ItemStore, JsonStore};
use serde_json::json;

#[tokio::test]
async fn json_store_writes_pretty_array_on_flush() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let store = JsonStore::new(tmp.path()).unwrap();
    store.store(&json!({"title": "A"})).await.unwrap();
    store.store(&json!({"title": "B"})).await.unwrap();
    store.flush().await.unwrap();

    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let items: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "A");
    assert_eq!(items[1]["title"], "B");
}
