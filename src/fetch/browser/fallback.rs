use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;

use crate::{
    error::KumoError,
    extract::Response,
    fetch::{FetchStatsSnapshot, Fetcher},
    middleware::FetchRequest,
};

use super::BrowserConfig;

type FallbackPredicate = dyn Fn(&Response) -> bool + Send + Sync;

pub struct BrowserFallbackConfig {
    pub(super) browser: BrowserConfig,
    should_fallback: Arc<FallbackPredicate>,
}

impl BrowserFallbackConfig {
    pub fn new(browser: BrowserConfig) -> Self {
        Self {
            browser,
            should_fallback: Arc::new(default_should_fallback),
        }
    }

    pub fn on_response(
        mut self,
        should_fallback: impl Fn(&Response) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.should_fallback = Arc::new(should_fallback);
        self
    }

    pub fn should_fallback(&self, response: &Response) -> bool {
        (self.should_fallback)(response)
    }

    pub(crate) fn split(self) -> (BrowserConfig, Arc<FallbackPredicate>) {
        (self.browser, self.should_fallback)
    }
}

pub struct BrowserFallbackFetcher {
    http: Arc<dyn Fetcher>,
    browser: Arc<dyn Fetcher>,
    should_fallback: Arc<FallbackPredicate>,
    counters: Arc<BrowserFallbackCounters>,
}

impl BrowserFallbackFetcher {
    pub fn new(
        http: impl Fetcher + 'static,
        browser: impl Fetcher + 'static,
        config: BrowserFallbackConfig,
    ) -> Self {
        let (_, should_fallback) = config.split();
        Self::from_parts(Arc::new(http), Arc::new(browser), should_fallback)
    }

    pub(crate) fn from_parts(
        http: Arc<dyn Fetcher>,
        browser: Arc<dyn Fetcher>,
        should_fallback: Arc<FallbackPredicate>,
    ) -> Self {
        Self {
            http,
            browser,
            should_fallback,
            counters: Arc::new(BrowserFallbackCounters::default()),
        }
    }
}

#[async_trait]
impl Fetcher for BrowserFallbackFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<Response, KumoError> {
        let http_response = self.http.fetch(request).await?;
        if !(self.should_fallback)(&http_response) {
            return Ok(http_response);
        }

        self.counters.fallbacks.fetch_add(1, Ordering::Relaxed);
        let mut fetch_stats = FetchStatsSnapshot {
            browser_fallbacks: 1,
            browser_fallback_successes: 0,
            browser_fallback_failures: 0,
        };
        match self.browser.fetch(request).await {
            Ok(response) => {
                self.counters.successes.fetch_add(1, Ordering::Relaxed);
                fetch_stats.browser_fallback_successes = 1;
                Ok(response.with_fetch_stats(fetch_stats))
            }
            Err(error) => {
                self.counters.failures.fetch_add(1, Ordering::Relaxed);
                fetch_stats.browser_fallback_failures = 1;
                tracing::debug!(
                    target: crate::logging::target::REQUEST,
                    event = "request.browser_fallback_failed",
                    url = %request.url(),
                    error = %error,
                    "request.browser_fallback_failed"
                );
                Ok(http_response.with_fetch_stats(fetch_stats))
            }
        }
    }

    fn stats(&self) -> FetchStatsSnapshot {
        let mut stats = self.http.stats();
        let browser_stats = self.browser.stats();
        stats.browser_fallbacks +=
            browser_stats.browser_fallbacks + self.counters.fallbacks.load(Ordering::Relaxed);
        stats.browser_fallback_successes += browser_stats.browser_fallback_successes
            + self.counters.successes.load(Ordering::Relaxed);
        stats.browser_fallback_failures += browser_stats.browser_fallback_failures
            + self.counters.failures.load(Ordering::Relaxed);
        stats
    }
}

#[derive(Default)]
struct BrowserFallbackCounters {
    fallbacks: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
}

fn default_should_fallback(response: &Response) -> bool {
    if response.status() != 200 {
        return false;
    }

    let Some(text) = response.text() else {
        return false;
    };
    let compact_len = text.trim().len();
    if compact_len == 0 {
        return true;
    }
    if compact_len > 2_048 {
        return false;
    }

    let lower = text.to_ascii_lowercase();
    let has_script = lower.contains("<script");
    let has_app_mount = ["id=\"root\"", "id='root'", "id=\"app\"", "id='app'"]
        .iter()
        .any(|needle| lower.contains(needle));
    let has_empty_mount = lower.contains("<div id=\"root\"></div>")
        || lower.contains("<div id='root'></div>")
        || lower.contains("<div id=\"app\"></div>")
        || lower.contains("<div id='app'></div>");
    let has_noscript_hint =
        lower.contains("enable javascript") || lower.contains("requires javascript");

    has_noscript_hint || (has_script && (has_app_mount || has_empty_mount))
}
