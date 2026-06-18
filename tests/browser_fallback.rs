#![cfg(feature = "browser")]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use kumo::{
    error::KumoError,
    extract::Response,
    fetch::{BrowserFallbackConfig, BrowserFallbackFetcher, FetchStatsSnapshot, Fetcher},
    middleware::FetchRequest,
};

#[test]
fn default_browser_fallback_detector_flags_empty_and_js_gated_html() {
    let cfg = BrowserFallbackConfig::new(kumo::fetch::BrowserConfig::headless());

    assert!(cfg.should_fallback(&Response::from_parts("https://example.com/empty", 200, "")));
    assert!(cfg.should_fallback(&Response::from_parts(
        "https://example.com/app",
        200,
        r#"<html><body><div id="root"></div><script src="/app.js"></script></body></html>"#
    )));
    assert!(!cfg.should_fallback(&Response::from_parts(
        "https://example.com/static",
        200,
        "<html><body><main><h1>Loaded</h1><p>Server-rendered content.</p></main></body></html>"
    )));
}

#[tokio::test]
async fn browser_fallback_fetcher_uses_browser_response_when_http_is_gated() {
    let http = StaticFetcher::new(
        200,
        r#"<div id="root"></div><script src="/app.js"></script>"#,
    );
    let browser = StaticFetcher::new(200, "<main><h1>Rendered</h1></main>");
    let fetcher = BrowserFallbackFetcher::new(
        http,
        browser,
        BrowserFallbackConfig::new(kumo::fetch::BrowserConfig::headless()),
    );

    let request = FetchRequest::new("https://example.com/app", 0);
    let response = fetcher.fetch(&request).await.unwrap();

    assert_eq!(response.text(), Some("<main><h1>Rendered</h1></main>"));
    assert_eq!(
        fetcher.stats(),
        FetchStatsSnapshot {
            browser_fallbacks: 1,
            browser_fallback_successes: 1,
            browser_fallback_failures: 0,
        }
    );
}

#[tokio::test]
async fn browser_fallback_fetcher_returns_http_response_when_browser_fails() {
    let http_body = r#"<div id="app"></div><script>window.__APP__ = true;</script>"#;
    let http = StaticFetcher::new(200, http_body);
    let browser = FailingFetcher;
    let fetcher = BrowserFallbackFetcher::new(
        http,
        browser,
        BrowserFallbackConfig::new(kumo::fetch::BrowserConfig::headless()),
    );

    let request = FetchRequest::new("https://example.com/app", 0);
    let response = fetcher.fetch(&request).await.unwrap();

    assert_eq!(response.text(), Some(http_body));
    assert_eq!(
        fetcher.stats(),
        FetchStatsSnapshot {
            browser_fallbacks: 1,
            browser_fallback_successes: 0,
            browser_fallback_failures: 1,
        }
    );
}

struct StaticFetcher {
    status: u16,
    body: &'static str,
    calls: Arc<AtomicUsize>,
}

impl StaticFetcher {
    fn new(status: u16, body: &'static str) -> Self {
        Self {
            status,
            body,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl Fetcher for StaticFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<Response, KumoError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(Response::from_parts(
            request.url().to_string(),
            self.status,
            self.body,
        ))
    }
}

struct FailingFetcher;

#[async_trait::async_trait]
impl Fetcher for FailingFetcher {
    async fn fetch(&self, _request: &FetchRequest) -> Result<Response, KumoError> {
        Err(KumoError::browser("browser fetch failed"))
    }
}
