pub mod autothrottle;
pub mod default_headers;
pub mod proxy;
pub mod rate_limit;
pub mod status_retry;
pub mod user_agent;

pub use autothrottle::AutoThrottle;
pub use default_headers::DefaultHeaders;
pub use proxy::{ProxyCircuitSnapshot, ProxyCircuitState, ProxyHealthSnapshot, ProxyRotator};
pub use rate_limit::RateLimiter;
pub use status_retry::StatusRetry;
pub use user_agent::UserAgentRotator;

use crate::{error::KumoError, extract::Response};
use reqwest::{
    Method,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

/// Shared selection strategy used by `UserAgentRotator` and `ProxyRotator`.
pub(super) enum RotationStrategy {
    RoundRobin(AtomicUsize),
    Random(AtomicUsize),
}

impl Clone for RotationStrategy {
    fn clone(&self) -> Self {
        match self {
            Self::RoundRobin(counter) => {
                Self::RoundRobin(AtomicUsize::new(counter.load(Ordering::Relaxed)))
            }
            Self::Random(state) => Self::Random(AtomicUsize::new(state.load(Ordering::Relaxed))),
        }
    }
}

impl RotationStrategy {
    pub(super) fn round_robin() -> Self {
        Self::RoundRobin(AtomicUsize::new(0))
    }

    pub(super) fn random() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize)
            .unwrap_or(42);
        Self::Random(AtomicUsize::new(seed | 1))
    }

    /// Return the index to use from a list of length `len`.
    pub(super) fn pick_index(&self, len: usize) -> usize {
        match self {
            Self::RoundRobin(counter) => counter.fetch_add(1, Ordering::Relaxed) % len,
            Self::Random(state) => {
                // XorShift pseudo-random — no external dependency needed.
                let mut x = state.load(Ordering::Relaxed);
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                state.store(x, Ordering::Relaxed);
                x % len
            }
        }
    }
}

/// A fetch-time HTTP request, passed through middleware before fetching.
pub struct FetchRequest {
    url: String,
    proxy_assignment_id: Option<u64>,
    pub method: Method,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
    pub depth: usize,
    /// Proxy URL set by `ProxyRotator` middleware (e.g. `"http://user:pass@host:port"`).
    /// The `HttpFetcher` reads this field to route the request through the specified proxy.
    pub proxy: Option<String>,
}

impl FetchRequest {
    pub fn new(url: impl Into<String>, depth: usize) -> Self {
        Self {
            url: url.into(),
            proxy_assignment_id: None,
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            depth,
            proxy: None,
        }
    }

    /// The URL this request will fetch.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub(super) fn proxy_assignment_id(&self) -> Option<u64> {
        self.proxy_assignment_id
    }

    pub(super) fn set_proxy_assignment_id(&mut self, id: Option<u64>) {
        self.proxy_assignment_id = id;
    }

    /// Mutable access to headers before the request is fetched.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Set or replace one header before the request is fetched.
    pub fn header(&mut self, name: HeaderName, value: HeaderValue) -> &mut Self {
        self.headers.insert(name, value);
        self
    }

    pub(crate) fn from_crawl_request(request: &crate::request::CrawlRequest, depth: usize) -> Self {
        Self {
            url: request.url().to_string(),
            proxy_assignment_id: None,
            method: request.method_ref().clone(),
            headers: request.headers().clone(),
            body: request.body_bytes().map(ToOwned::to_owned),
            depth,
            proxy: None,
        }
    }
}

/// Wraps the fetch cycle with pre/post-request hooks.
/// Multiple middleware are applied in registration order.
#[async_trait::async_trait]
pub trait Middleware: Send + Sync {
    /// Called before the HTTP request is sent.
    /// Modify `request` in place (e.g., inject headers, enforce rate limits).
    async fn before_request(&self, request: &mut FetchRequest) -> Result<(), KumoError>;

    /// Called after a successful HTTP response.
    /// Modify `response` in place, or return an error to trigger the spider's error policy.
    async fn after_response(&self, _response: &mut Response) -> Result<(), KumoError> {
        Ok(())
    }

    /// Called after a successful HTTP response with its originating request.
    ///
    /// The default delegates to [`Middleware::after_response`] so existing
    /// middleware implementations remain compatible.
    async fn after_response_with_request(
        &self,
        _request: &FetchRequest,
        response: &mut Response,
    ) -> Result<(), KumoError> {
        self.after_response(response).await
    }

    /// Called when one fetch attempt fails before producing a response.
    ///
    /// Unlike [`Middleware::on_error`], this runs for each failed fetch attempt,
    /// including attempts that the engine may retry. The default does nothing.
    async fn on_fetch_error(&self, _request: &FetchRequest, _error: &KumoError) {}

    /// Called when a URL permanently fails (after all retries are exhausted).
    /// Use this to log failures, mark proxies as bad, emit metrics, etc.
    /// Default implementation does nothing.
    async fn on_error(&self, _url: &str, _error: &KumoError) {}

    /// Optional retry delay hint for an error observed by this middleware.
    ///
    /// Middleware can use this to pass server-provided backoff information,
    /// such as `Retry-After`, to the engine without changing `KumoError`.
    fn retry_delay(&self, _url: &str, _error: &KumoError) -> Option<Duration> {
        None
    }
}
