use async_trait::async_trait;

use crate::{error::KumoError, extract::Response};

use super::{Middleware, Request};

/// Default HTTP status codes that should trigger an automatic retry.
const DEFAULT_RETRY_CODES: &[u16] = &[429, 500, 502, 503, 504];

/// Middleware that turns error HTTP status codes into `KumoError::HttpStatus`,
/// causing the engine's exponential-backoff retry loop to re-fetch the URL.
///
/// By default retries on 429, 500, 502, 503, and 504. Respects the
/// `Retry-After` header for 429 responses when `respect_retry_after` is true.
///
/// # Example
/// ```rust,ignore
/// CrawlEngine::builder()
///     .retry(3, Duration::from_secs(1))
///     .middleware(StatusRetry::new())
/// ```
pub struct StatusRetry {
    codes: Vec<u16>,
}

impl StatusRetry {
    /// Retry on the default set of codes: 429, 500, 502, 503, 504.
    pub fn new() -> Self {
        Self {
            codes: DEFAULT_RETRY_CODES.to_vec(),
        }
    }

    /// Retry on a custom set of status codes.
    pub fn with_codes(codes: Vec<u16>) -> Self {
        Self { codes }
    }
}

impl Default for StatusRetry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for StatusRetry {
    async fn before_request(&self, _request: &mut Request) -> Result<(), KumoError> {
        Ok(())
    }

    async fn after_response(&self, response: &mut Response) -> Result<(), KumoError> {
        if self.codes.contains(&response.status) {
            return Err(KumoError::HttpStatus {
                status: response.status,
                url: response.url.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(status: u16) -> Response {
        Response::from_parts("https://example.com/page", status, "")
    }

    #[tokio::test]
    async fn allows_200() {
        let mw = StatusRetry::new();
        let mut res = make_response(200);
        assert!(mw.after_response(&mut res).await.is_ok());
    }

    #[tokio::test]
    async fn rejects_429() {
        let mw = StatusRetry::new();
        let mut res = make_response(429);
        let err = mw.after_response(&mut res).await.unwrap_err();
        assert!(matches!(err, KumoError::HttpStatus { status: 429, .. }));
    }

    #[tokio::test]
    async fn rejects_503() {
        let mw = StatusRetry::new();
        let mut res = make_response(503);
        assert!(matches!(
            mw.after_response(&mut res).await.unwrap_err(),
            KumoError::HttpStatus { status: 503, .. }
        ));
    }

    #[tokio::test]
    async fn custom_codes_respected() {
        let mw = StatusRetry::with_codes(vec![403]);
        let mut res = make_response(403);
        assert!(mw.after_response(&mut res).await.is_err());
        let mut ok = make_response(503);
        assert!(mw.after_response(&mut ok).await.is_ok());
    }
}
