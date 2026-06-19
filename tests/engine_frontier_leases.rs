use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

use kumo::{
    CrawlRequest,
    engine::CrawlEngine,
    error::KumoError,
    extract::Response,
    fetch::MockFetcher,
    frontier::{DeadLetterReason, Frontier, FrontierLease, FrontierLeaseId},
    request::FrontierRequest,
    spider::{Output, Spider},
    store::StdoutStore,
};
use tokio::sync::Mutex;

struct LeaseTrackingFrontier {
    queue: Mutex<VecDeque<FrontierRequest>>,
    leased: Mutex<Option<FrontierRequest>>,
    acked: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    dead_lettered: Arc<AtomicBool>,
}

impl LeaseTrackingFrontier {
    fn new(
        acked: Arc<AtomicBool>,
        released: Arc<AtomicBool>,
        dead_lettered: Arc<AtomicBool>,
    ) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            leased: Mutex::new(None),
            acked,
            released,
            dead_lettered,
        }
    }
}

#[async_trait::async_trait]
impl Frontier for LeaseTrackingFrontier {
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

    async fn push_request_force(&self, queued: FrontierRequest) {
        self.queue.lock().await.push_back(queued);
    }

    async fn pop_request(&self) -> Option<FrontierRequest> {
        self.queue.lock().await.pop_front()
    }

    async fn lease_request(&self, ttl: Duration) -> Option<FrontierLease> {
        let queued = self.queue.lock().await.pop_front()?;
        *self.leased.lock().await = Some(queued.clone());
        Some(FrontierLease::new(
            FrontierLeaseId::new("lease-1"),
            queued,
            Some(SystemTime::now() + ttl),
            1,
        ))
    }

    async fn ack_lease(&self, _lease_id: &FrontierLeaseId) -> Result<(), KumoError> {
        self.acked.store(true, Ordering::SeqCst);
        *self.leased.lock().await = None;
        Ok(())
    }

    async fn release_lease(&self, _lease_id: &FrontierLeaseId) -> Result<(), KumoError> {
        self.released.store(true, Ordering::SeqCst);
        if let Some(queued) = self.leased.lock().await.take() {
            self.queue.lock().await.push_back(queued);
        }
        Ok(())
    }

    async fn dead_letter(
        &self,
        _lease_id: &FrontierLeaseId,
        _reason: DeadLetterReason,
    ) -> Result<(), KumoError> {
        self.dead_lettered.store(true, Ordering::SeqCst);
        *self.leased.lock().await = None;
        Ok(())
    }

    fn supports_leases(&self) -> bool {
        true
    }

    async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}

struct LeaseSpider;

#[async_trait::async_trait]
impl Spider for LeaseSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "lease-spider"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://example.com".to_string()]
    }

    async fn parse(&self, _response: &Response) -> Result<Output<Self::Item>, KumoError> {
        Ok(Output::new())
    }
}

#[tokio::test]
async fn engine_acks_leased_request_after_success() {
    let acked = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let dead_lettered = Arc::new(AtomicBool::new(false));
    let frontier =
        LeaseTrackingFrontier::new(acked.clone(), released.clone(), dead_lettered.clone());
    let fetcher = MockFetcher::new().with_response("https://example.com", 200, "<h1>ok</h1>");

    CrawlEngine::builder()
        .frontier(frontier)
        .fetcher(fetcher)
        .respect_robots_txt(false)
        .store(StdoutStore)
        .run(LeaseSpider)
        .await
        .unwrap();

    assert!(acked.load(Ordering::SeqCst));
    assert!(!released.load(Ordering::SeqCst));
    assert!(!dead_lettered.load(Ordering::SeqCst));
}
