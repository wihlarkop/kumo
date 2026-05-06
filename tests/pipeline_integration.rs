mod support;

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    pipeline::RequireFields,
    spider::{Output, Spider},
};
use support::VecStore;

#[tokio::test]
async fn pipeline_drops_items_missing_required_field() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><p>no title here</p></body></html>")
        .create_async()
        .await;

    struct NoTitleSpider(String);

    #[async_trait::async_trait]
    impl Spider for NoTitleSpider {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "no-title"
        }

        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }

        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new().item(serde_json::json!({ "body": "hello" })))
        }
    }

    let store = VecStore::default();
    let stats = CrawlEngine::builder()
        .respect_robots_txt(false)
        .pipeline(RequireFields::new(&["title"]))
        .store(store.clone())
        .run(NoTitleSpider(server.url()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(
        stats.items_scraped, 0,
        "pipeline should have dropped the item"
    );
    assert!(store.collected().is_empty());
}
