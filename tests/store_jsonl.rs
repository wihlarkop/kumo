use kumo::store::{ItemStore, JsonlStore};
use serde_json::json;

#[tokio::test]
async fn jsonl_store_writes_one_object_per_line() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let store = JsonlStore::new(tmp.path()).unwrap();
    store.store(&json!({"title": "A"})).await.unwrap();
    store.store(&json!({"title": "B"})).await.unwrap();
    store.flush().await.unwrap();

    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["title"],
        "A"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[1]).unwrap()["title"],
        "B"
    );
}
