pub mod cache;
pub(crate) mod client_policy;
pub mod http;
pub mod mock;

#[cfg(feature = "browser")]
pub mod browser;

#[cfg(feature = "stealth")]
pub mod stealth_http;

pub use cache::CachingFetcher;
pub use http::HttpFetcher;
pub use mock::MockFetcher;

#[cfg(feature = "browser")]
pub use browser::{BrowserConfig, BrowserFallbackConfig, BrowserFallbackFetcher, BrowserFetcher};

#[cfg(feature = "stealth")]
pub use stealth_http::{StealthHttpFetcher, StealthProfile};

use crate::{error::KumoError, extract::Response, middleware::FetchRequest};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FetchStatsSnapshot {
    pub browser_fallbacks: u64,
    pub browser_fallback_successes: u64,
    pub browser_fallback_failures: u64,
}

/// Abstracts over different HTTP backends.
#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    async fn fetch(&self, request: &FetchRequest) -> Result<Response, KumoError>;

    fn stats(&self) -> FetchStatsSnapshot {
        FetchStatsSnapshot::default()
    }
}

/// Returns `true` if the Content-Type indicates a text body (should be decoded as UTF-8).
/// Defaults to `true` when the header is absent.
pub(crate) fn is_text_content_type(content_type: Option<&str>) -> bool {
    match content_type {
        Some(ct) => ct.starts_with("text/") || ct.contains("application/json"),
        None => true,
    }
}
