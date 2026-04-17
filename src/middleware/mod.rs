pub mod autothrottle;
pub mod default_headers;
pub mod rate_limit;

pub use autothrottle::AutoThrottle;
pub use default_headers::DefaultHeaders;
pub use rate_limit::RateLimiter;

use crate::{error::KumoError, extract::Response};
use reqwest::header::HeaderMap;

/// A pending HTTP request, passed through the middleware chain before fetching.
pub struct Request {
    pub url: String,
    pub headers: HeaderMap,
    pub depth: usize,
}

impl Request {
    pub fn new(url: impl Into<String>, depth: usize) -> Self {
        Self {
            url: url.into(),
            headers: HeaderMap::new(),
            depth,
        }
    }
}

/// Wraps the fetch cycle with pre/post-request hooks.
/// Multiple middleware are applied in registration order.
#[async_trait::async_trait]
pub trait Middleware: Send + Sync {
    /// Called before the HTTP request is sent.
    /// Modify `request` in place (e.g., inject headers, enforce rate limits).
    async fn before_request(&self, request: &mut Request) -> Result<(), KumoError>;

    /// Called after a successful HTTP response.
    /// Modify `response` in place, or return an error to trigger the spider's error policy.
    async fn after_response(&self, _response: &mut Response) -> Result<(), KumoError> {
        Ok(())
    }
}
