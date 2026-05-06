use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    middleware::DefaultHeaders,
    spider::{Output, Spider},
    store::StdoutStore,
};

#[tokio::test]
async fn middleware_injects_custom_user_agent() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>ok</h1></body></html>")
        .match_header("user-agent", "test-bot/1.0")
        .create_async()
        .await;

    struct AgentSpider(String);

    #[async_trait::async_trait]
    impl Spider for AgentSpider {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "agent"
        }

        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }

        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new())
        }
    }

    let stats = CrawlEngine::builder()
        .respect_robots_txt(false)
        .middleware(DefaultHeaders::new().user_agent("test-bot/1.0"))
        .store(StdoutStore)
        .run(AgentSpider(server.url()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.errors, 0);
    mock.assert_async().await;
}
