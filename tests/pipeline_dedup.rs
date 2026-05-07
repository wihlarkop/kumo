use kumo::pipeline::{DropDuplicates, Pipeline};
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

#[tokio::test]
async fn persistence_rejects_duplicates_after_reload() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    {
        let p = DropDuplicates::with_persistence("url", &path).unwrap();
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

    let p2 = DropDuplicates::with_persistence("url", &path).unwrap();
    assert!(
        p2.process(json!({"url": "https://a.com"}))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        p2.process(json!({"url": "https://b.com"}))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        p2.process(json!({"url": "https://c.com"}))
            .await
            .unwrap()
            .is_some()
    );
}
