#![cfg(feature = "cloud")]

use std::sync::Arc;

use kumo::store::{CloudFormat, CloudStore, ItemStore};
use object_store::{ObjectStoreExt, memory::InMemory, path::Path as StorePath};
use serde_json::json;

fn mem_store() -> Arc<InMemory> {
    Arc::new(InMemory::new())
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
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["title"],
        "A"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[1]).unwrap()["title"],
        "B"
    );
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
    assert!(mem.get(&path).await.is_err());
}
