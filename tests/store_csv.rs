use kumo::store::{CsvStore, ItemStore};
use serde_json::json;
use tempfile::NamedTempFile;

fn make_store_at(path: &std::path::Path) -> CsvStore {
    CsvStore::new(path).unwrap()
}

#[tokio::test]
async fn auto_headers_and_rows() {
    let tmp = NamedTempFile::new().unwrap();
    let store = make_store_at(tmp.path());

    store
        .store(&json!({"title": "Hello", "url": "https://example.com"}))
        .await
        .unwrap();
    store
        .store(&json!({"title": "World", "url": "https://example.org"}))
        .await
        .unwrap();
    store.flush().await.unwrap();

    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines[0], "title,url");
    assert_eq!(lines[1], "Hello,https://example.com");
    assert_eq!(lines[2], "World,https://example.org");
}

#[tokio::test]
async fn commas_are_quoted() {
    let tmp = NamedTempFile::new().unwrap();
    let store = make_store_at(tmp.path());

    store
        .store(&json!({"value": "one, two, three"}))
        .await
        .unwrap();
    store.flush().await.unwrap();

    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines[0], "value");
    assert_eq!(lines[1], "\"one, two, three\"");
}

#[tokio::test]
async fn interior_quotes_are_doubled() {
    let tmp = NamedTempFile::new().unwrap();
    let store = make_store_at(tmp.path());

    store
        .store(&json!({"value": r#"say "hello""#}))
        .await
        .unwrap();
    store.flush().await.unwrap();

    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines[1], "\"say \"\"hello\"\"\"");
}

#[tokio::test]
async fn with_headers_sets_column_order() {
    let tmp = NamedTempFile::new().unwrap();
    let store = CsvStore::with_headers(tmp.path(), &["url", "title"]).unwrap();

    store
        .store(&json!({"title": "Hello", "url": "https://example.com"}))
        .await
        .unwrap();
    store.flush().await.unwrap();

    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines[0], "url,title");
    assert_eq!(lines[1], "https://example.com,Hello");
}

#[tokio::test]
async fn missing_key_becomes_empty_cell() {
    let tmp = NamedTempFile::new().unwrap();
    let store = CsvStore::with_headers(tmp.path(), &["title", "price"]).unwrap();

    store.store(&json!({"title": "Widget"})).await.unwrap();
    store.flush().await.unwrap();

    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines[1], "Widget,");
}
