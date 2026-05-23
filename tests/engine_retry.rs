mod support;

use kumo::{
    engine::CrawlEngine,
    error::{ErrorPolicy, KumoError},
    extract::Response,
    fetch::Fetcher,
    middleware::{FetchRequest, StatusRetry},
    retry::RetryPolicy,
    spider::{Output, Spider},
    store::StdoutStore,
};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use std::time::{Duration, Instant};
use support::VecStore;

struct SequentialFetcher {
    calls: Arc<AtomicU32>,
    fail_times: u32,
}

#[async_trait::async_trait]
impl Fetcher for SequentialFetcher {
    async fn fetch(&self, req: &FetchRequest) -> Result<Response, KumoError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let status = if n < self.fail_times { 503 } else { 200 };
        Ok(Response::from_parts(
            req.url(),
            status,
            "<html><body><h1>ok</h1></body></html>",
        ))
    }
}

#[tokio::test]
async fn status_retry_retries_on_429_and_succeeds() {
    let mut server = mockito::Server::new_async().await;
    let _m1 = server
        .mock("GET", "/")
        .with_status(429)
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>ok</h1></body></html>")
        .create_async()
        .await;

    struct RetrySpider(String);

    #[async_trait::async_trait]
    impl Spider for RetrySpider {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "retry"
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
        .retry(1, std::time::Duration::from_millis(1))
        .middleware(StatusRetry::new())
        .store(StdoutStore)
        .run(RetrySpider(server.url()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.errors, 0);
}

#[tokio::test]
async fn retry_policy_exhausted_counts_as_single_error() {
    let calls = Arc::new(AtomicU32::new(0));
    let fetcher = SequentialFetcher {
        calls: calls.clone(),
        fail_times: 100,
    };

    struct S;

    #[async_trait::async_trait]
    impl Spider for S {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "retry-exhaust"
        }

        fn start_urls(&self) -> Vec<String> {
            vec!["https://example.com/".into()]
        }

        async fn parse(&self, _r: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new())
        }
    }

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .middleware(StatusRetry::new())
        .retry_policy(
            RetryPolicy::new(3)
                .base_delay(std::time::Duration::from_millis(1))
                .max_delay(std::time::Duration::from_millis(4)),
        )
        .respect_robots_txt(false)
        .store(StdoutStore)
        .run(S)
        .await
        .unwrap();

    assert_eq!(stats.errors, 1, "all retries exhausted = one error");
    assert_eq!(stats.retries, 3);
    assert_eq!(stats.retry_exhausted, 1);
    assert_eq!(stats.domains["example.com"].retries, 3);
    assert_eq!(stats.domains["example.com"].retry_exhausted, 1);
    assert_eq!(stats.pages_crawled, 0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "initial + 3 retries = 4 fetches"
    );
}

#[tokio::test]
async fn retry_policy_succeeds_on_later_attempt() {
    let calls = Arc::new(AtomicU32::new(0));
    let fetcher = SequentialFetcher {
        calls: calls.clone(),
        fail_times: 2,
    };

    struct S;

    #[async_trait::async_trait]
    impl Spider for S {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "retry-success"
        }

        fn start_urls(&self) -> Vec<String> {
            vec!["https://example.com/".into()]
        }

        async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
            let title = res.css("h1").first().map(|e| e.text()).unwrap_or_default();
            Ok(Output::new().item(serde_json::json!({ "title": title })))
        }
    }

    let store = VecStore::default();
    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .middleware(StatusRetry::new())
        .retry_policy(
            RetryPolicy::new(3)
                .base_delay(std::time::Duration::from_millis(1))
                .max_delay(std::time::Duration::from_millis(4)),
        )
        .respect_robots_txt(false)
        .store(store.clone())
        .run(S)
        .await
        .unwrap();

    assert_eq!(stats.errors, 0, "succeeded after 2 failures");
    assert_eq!(stats.retries, 2);
    assert_eq!(stats.retry_exhausted, 0);
    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.items_scraped, 1);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "2 failures + 1 success = 3 fetches"
    );
    assert_eq!(store.collected()[0]["title"], "ok");
}

#[tokio::test]
async fn error_policy_retry_exhaustion_is_observable() {
    struct ParseRetrySpider(String);

    #[async_trait::async_trait]
    impl Spider for ParseRetrySpider {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "parse-retry-exhaust"
        }

        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }

        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Err(KumoError::parse_msg("parse failed"))
        }

        fn on_error(&self, _url: &str, _err: &KumoError) -> ErrorPolicy {
            ErrorPolicy::Retry(1)
        }
    }

    let url = "https://example.com/parse";
    let fetcher = kumo::fetch::MockFetcher::new().with_response(url, 200, "<h1>bad</h1>");

    let stats = CrawlEngine::builder()
        .fetcher(fetcher)
        .respect_robots_txt(false)
        .store(StdoutStore)
        .run(ParseRetrySpider(url.to_string()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 0);
    assert_eq!(stats.errors, 2);
    assert_eq!(stats.retries, 1);
    assert_eq!(stats.retry_exhausted, 1);
    assert_eq!(stats.domains["example.com"].retry_exhausted, 1);
}

#[tokio::test]
async fn status_retry_uses_retry_after_before_requeueing() {
    let mut server = mockito::Server::new_async().await;
    let _m1 = server
        .mock("GET", "/")
        .with_status(429)
        .with_header("retry-after", "1")
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>ok</h1></body></html>")
        .create_async()
        .await;

    struct RetrySpider(String);

    #[async_trait::async_trait]
    impl Spider for RetrySpider {
        type Item = serde_json::Value;

        fn name(&self) -> &str {
            "retry-after"
        }

        fn start_urls(&self) -> Vec<String> {
            vec![self.0.clone()]
        }

        async fn parse(&self, _res: &Response) -> Result<Output<Self::Item>, KumoError> {
            Ok(Output::new())
        }
    }

    let started = Instant::now();
    let stats = CrawlEngine::builder()
        .respect_robots_txt(false)
        .retry_policy(
            RetryPolicy::new(1)
                .base_delay(Duration::from_millis(1))
                .max_delay(Duration::from_secs(2)),
        )
        .middleware(StatusRetry::new())
        .store(StdoutStore)
        .run(RetrySpider(server.url()))
        .await
        .unwrap();

    assert_eq!(stats.pages_crawled, 1);
    assert_eq!(stats.errors, 0);
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "retry should respect Retry-After delay"
    );
}
