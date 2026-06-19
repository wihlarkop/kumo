use std::time::Duration;

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::MockFetcher,
    spider::{Output, Spider},
    store::StdoutStore,
};
use tempfile::tempdir;

struct CheckpointSpider;

#[async_trait::async_trait]
impl Spider for CheckpointSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "checkpoint-spider"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://example.com".to_string()]
    }

    async fn parse(&self, _response: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new())
    }
}

#[tokio::test]
async fn engine_writes_final_stats_checkpoint() {
    let dir = tempdir().unwrap();
    let checkpoint = dir.path().join("crawl-report.json");
    let fetcher = MockFetcher::new().with_response("https://example.com", 200, "<h1>ok</h1>");

    CrawlEngine::builder()
        .fetcher(fetcher)
        .respect_robots_txt(false)
        .store(StdoutStore)
        .stats_checkpoint_interval(&checkpoint, Duration::from_secs(60))
        .run(CheckpointSpider)
        .await
        .unwrap();

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(checkpoint).unwrap()).unwrap();
    assert_eq!(report["pages_crawled"], 1);
    assert_eq!(report["stop_reason"], "frontier_exhausted");
}
