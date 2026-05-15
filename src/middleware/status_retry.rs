use async_trait::async_trait;
use regex::Regex;

use crate::{error::KumoError, extract::Response};

use super::{FetchRequest, Middleware};
use reqwest::header::{HeaderValue, RETRY_AFTER};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime},
};

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
/// Default - retry the standard transient codes for every URL:
/// ```rust,ignore
/// .middleware(StatusRetry::new())
/// ```
///
/// Per-pattern - retry 404 on dynamic API paths, never retry static assets:
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
    retry_after: Mutex<HashMap<(String, u16), Duration>>,
}

impl StatusRetry {
    /// Retry on the default set of codes: 429, 500, 502, 503, 504.
    pub fn new() -> Self {
        Self {
            codes: DEFAULT_RETRY_CODES.to_vec(),
            patterns: Vec::new(),
            retry_after: Mutex::new(HashMap::new()),
        }
    }

    /// Retry on a custom global set of status codes (no per-URL patterns).
    pub fn with_codes(codes: Vec<u16>) -> Self {
        Self {
            codes,
            patterns: Vec::new(),
            retry_after: Mutex::new(HashMap::new()),
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

    fn error_for_response(&self, response: &Response) -> KumoError {
        self.record_retry_after(response);
        KumoError::HttpStatus {
            status: response.status(),
            url: response.url().to_string(),
        }
    }

    fn record_retry_after(&self, response: &Response) {
        let key = (response.url().to_string(), response.status());
        let delay = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(parse_retry_after);
        let mut retry_after = self
            .retry_after
            .lock()
            .expect("status retry delay mutex poisoned");

        if let Some(delay) = delay {
            retry_after.insert(key, delay);
        } else {
            retry_after.remove(&key);
        }
    }
}

fn parse_retry_after(value: &HeaderValue) -> Option<Duration> {
    let value = value.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let retry_at = httpdate::parse_http_date(value).ok()?;
    match retry_at.duration_since(SystemTime::now()) {
        Ok(delay) => Some(delay),
        Err(_) => Some(Duration::ZERO),
    }
}

impl std::fmt::Debug for StatusRetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusRetry")
            .field("global_codes", &self.codes)
            .field("pattern_rules", &self.patterns.len())
            .field(
                "retry_after_hints",
                &self.retry_after.lock().map(|hints| hints.len()).ok(),
            )
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
    async fn before_request(&self, _request: &mut FetchRequest) -> Result<(), KumoError> {
        Ok(())
    }

    async fn after_response(&self, response: &mut Response) -> Result<(), KumoError> {
        // Pattern rules are checked first; the first match takes precedence.
        for (pattern, codes) in &self.patterns {
            if pattern.is_match(response.url()) {
                return if codes.contains(&response.status()) {
                    Err(self.error_for_response(response))
                } else {
                    Ok(()) // Pattern matched but status not in this rule's retry set.
                };
            }
        }

        // No pattern matched - fall back to global codes.
        if self.codes.contains(&response.status()) {
            return Err(self.error_for_response(response));
        }
        Ok(())
    }

    fn retry_delay(&self, _url: &str, error: &KumoError) -> Option<Duration> {
        let KumoError::HttpStatus { status, url } = error else {
            return None;
        };

        self.retry_after
            .lock()
            .expect("status retry delay mutex poisoned")
            .remove(&(url.clone(), *status))
    }
}
