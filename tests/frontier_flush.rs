use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use kumo::{
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::MockFetcher,
    frontier::Frontier,
    request::{CrawlRequest, FrontierRequest},
    spider::{Output, Spider},
    store::StdoutStore,
};
use tokio::sync::Mutex;

struct FlushTrackingFrontier {
    queue: Mutex<VecDeque<FrontierRequest>>,
    flushed: Arc<AtomicBool>,
}

impl FlushTrackingFrontier {
    fn new(flushed: Arc<AtomicBool>) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            flushed,
        }
    }
}

#[async_trait::async_trait]
impl Frontier for FlushTrackingFrontier {
    async fn push(&self, url: String, depth: usize) -> bool {
        self.push_request(CrawlRequest::get(url), depth).await
    }

    async fn push_force(&self, url: String, depth: usize, retry_count: u32) {
        self.push_request_force(FrontierRequest::new(
            CrawlRequest::get(url),
            depth,
            retry_count,
        ))
        .await;
    }

    async fn pop(&self) -> Option<(String, usize, u32)> {
        self.pop_request().await.map(|queued| {
            (
                queued.request().url().to_string(),
                queued.depth(),
                queued.retry_count(),
            )
        })
    }

    async fn push_request(&self, request: CrawlRequest, depth: usize) -> bool {
        self.queue
            .lock()
            .await
            .push_back(FrontierRequest::new(request, depth, 0));
        true
    }

    async fn pop_request(&self) -> Option<FrontierRequest> {
        self.queue.lock().await.pop_front()
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }

    async fn flush(&self) -> Result<(), KumoError> {
        self.flushed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct FlushSpider;

#[async_trait::async_trait]
impl Spider for FlushSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "flush"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://example.com".to_string()]
    }

    async fn parse(&self, _response: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new())
    }
}

#[tokio::test]
async fn engine_flushes_frontier_when_crawl_finishes() {
    let flushed = Arc::new(AtomicBool::new(false));
    let frontier = FlushTrackingFrontier::new(flushed.clone());
    let fetcher = MockFetcher::new().with_response("https://example.com", 200, "<h1>ok</h1>");

    CrawlEngine::builder()
        .frontier(frontier)
        .fetcher(fetcher)
        .respect_robots_txt(false)
        .store(StdoutStore)
        .run(FlushSpider)
        .await
        .unwrap();

    assert!(flushed.load(Ordering::SeqCst));
}
