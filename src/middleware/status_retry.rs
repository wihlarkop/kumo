use async_trait::async_trait;
use regex::Regex;

use crate::{error::KumoError, extract::Response};

use super::{Middleware, Request};

/// Default HTTP status codes that should trigger an automatic retry.
const DEFAULT_RETRY_CODES: &[u16] = &[429, 500, 502, 503, 504];

/// Middleware that turns error HTTP status codes into `KumoError::HttpStatus`,
/// causing the engine's exponential-backoff retry loop to re-fetch the URL.
///
/// By default retries on 429, 500, 502, 503, and 504. Call `.for_pattern()`
/// to configure per-URL-pattern codes that override the global set.
///
/// # Examples
///
/// Default — retry the standard transient codes for every URL:
/// ```rust,ignore
/// .middleware(StatusRetry::new())
/// ```
///
/// Per-pattern — retry 404 on dynamic API paths, never retry static assets:
/// ```rust,ignore
/// .middleware(
///     StatusRetry::new()
///         .for_pattern(r"^https://api\.example\.com/", vec![404, 500, 503])
///         .for_pattern(r"\.(js|css|png|jpg|woff2?)$", vec![])
/// )
/// ```
pub struct StatusRetry {
    codes: Vec<u16>,
    patterns: Vec<(Regex, Vec<u16>)>,
}

impl StatusRetry {
    /// Retry on the default set of codes: 429, 500, 502, 503, 504.
    pub fn new() -> Self {
        Self {
            codes: DEFAULT_RETRY_CODES.to_vec(),
            patterns: Vec::new(),
        }
    }

    /// Retry on a custom global set of status codes (no per-URL patterns).
    pub fn with_codes(codes: Vec<u16>) -> Self {
        Self {
            codes,
            patterns: Vec::new(),
        }
    }

    /// Add a per-URL pattern rule.
    ///
    /// The first matching pattern wins. If `codes` is empty, matching URLs
    /// are never retried regardless of status (opt-out). If no pattern
    /// matches a URL, the global `codes` apply.
    ///
    /// `pattern` is a regular expression matched against the full URL.
    /// Panics if `pattern` is not a valid regex.
    pub fn for_pattern(mut self, pattern: &str, codes: Vec<u16>) -> Self {
        let re = Regex::new(pattern)
            .unwrap_or_else(|e| panic!("invalid StatusRetry pattern '{pattern}': {e}"));
        self.patterns.push((re, codes));
        self
    }
}

impl std::fmt::Debug for StatusRetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusRetry")
            .field("global_codes", &self.codes)
            .field("pattern_rules", &self.patterns.len())
            .finish()
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
        // Pattern rules are checked first; the first match takes precedence.
        for (pattern, codes) in &self.patterns {
            if pattern.is_match(response.url()) {
                return if codes.contains(&response.status()) {
                    Err(KumoError::HttpStatus {
                        status: response.status(),
                        url: response.url().to_string(),
                    })
                } else {
                    Ok(()) // Pattern matched but status not in this rule's retry set.
                };
            }
        }
        // No pattern matched — fall back to global codes.
        if self.codes.contains(&response.status()) {
            return Err(KumoError::HttpStatus {
                status: response.status(),
                url: response.url().to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(url: &str, status: u16) -> Response {
        Response::from_parts(url, status, "")
    }

    #[tokio::test]
    async fn allows_200() {
        let mw = StatusRetry::new();
        let mut res = make_response("https://example.com/page", 200);
        assert!(mw.after_response(&mut res).await.is_ok());
    }

    #[tokio::test]
    async fn rejects_429() {
        let mw = StatusRetry::new();
        let mut res = make_response("https://example.com/page", 429);
        let err = mw.after_response(&mut res).await.unwrap_err();
        assert!(matches!(err, KumoError::HttpStatus { status: 429, .. }));
    }

    #[tokio::test]
    async fn rejects_503() {
        let mw = StatusRetry::new();
        let mut res = make_response("https://example.com/page", 503);
        assert!(matches!(
            mw.after_response(&mut res).await.unwrap_err(),
            KumoError::HttpStatus { status: 503, .. }
        ));
    }

    #[tokio::test]
    async fn custom_codes_respected() {
        let mw = StatusRetry::with_codes(vec![403]);
        let mut res = make_response("https://example.com/page", 403);
        assert!(mw.after_response(&mut res).await.is_err());
        let mut ok = make_response("https://example.com/page", 503);
        assert!(mw.after_response(&mut ok).await.is_ok());
    }

    #[tokio::test]
    async fn pattern_overrides_global_for_matching_url() {
        let mw = StatusRetry::new().for_pattern(r"^https://example\.com/api/", vec![404]);
        let mut api_404 = make_response("https://example.com/api/users", 404);
        assert!(matches!(
            mw.after_response(&mut api_404).await.unwrap_err(),
            KumoError::HttpStatus { status: 404, .. }
        ));
    }

    #[tokio::test]
    async fn pattern_opts_out_for_matching_url() {
        let mw = StatusRetry::new().for_pattern(r"\.(js|css|png)$", vec![]);
        let mut static_500 = make_response("https://example.com/style.css", 500);
        assert!(mw.after_response(&mut static_500).await.is_ok());
    }

    #[tokio::test]
    async fn global_codes_apply_when_no_pattern_matches() {
        let mw = StatusRetry::new().for_pattern(r"^https://example\.com/api/", vec![404]);
        let mut other_500 = make_response("https://example.com/page", 500);
        assert!(matches!(
            mw.after_response(&mut other_500).await.unwrap_err(),
            KumoError::HttpStatus { status: 500, .. }
        ));
    }

    #[tokio::test]
    async fn first_matching_pattern_wins() {
        let mw = StatusRetry::new()
            .for_pattern(r"/api/", vec![404])
            .for_pattern(r"/api/users", vec![]);
        let mut res = make_response("https://example.com/api/users", 404);
        assert!(mw.after_response(&mut res).await.is_err());
    }
}
