use std::time::Duration;

use crate::error::KumoError;

/// Configures how the engine retries failed fetch attempts.
///
/// # Example
/// ```rust,ignore
/// CrawlEngine::builder()
///     .retry_policy(
///         RetryPolicy::new(3)
///             .base_delay(Duration::from_millis(200))
///             .max_delay(Duration::from_secs(30))
///             .jitter(true)
///             .on_status(429)
///             .on_status(503),
///     )
/// ```
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub(crate) max_attempts: u32,
    pub(crate) base_delay: Duration,
    pub(crate) max_delay: Duration,
    pub(crate) jitter: bool,
    /// Empty = retry any `HttpStatus` or `Fetch` error.
    /// Non-empty = only retry `HttpStatus` where the code is in this list.
    pub(crate) retriable_statuses: Vec<u16>,
}

impl RetryPolicy {
    /// Create a policy with `max_attempts` retries, 500ms base delay, 60s cap, no jitter.
    ///
    /// `max_attempts` is the number of *retries* — total fetch calls = `max_attempts + 1`.
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            jitter: false,
            retriable_statuses: Vec::new(),
        }
    }

    pub fn base_delay(mut self, d: Duration) -> Self {
        self.base_delay = d;
        self
    }

    pub fn max_delay(mut self, d: Duration) -> Self {
        self.max_delay = d;
        self
    }

    /// Add ≤25% random jitter to each delay so concurrent retries don't thundering-herd.
    pub fn jitter(mut self, enabled: bool) -> Self {
        self.jitter = enabled;
        self
    }

    /// Only retry when the HTTP response status code matches.
    /// Call multiple times to allow several codes.
    ///
    /// If never called, retries on any `KumoError::HttpStatus` or `KumoError::Fetch`.
    pub fn on_status(mut self, status: u16) -> Self {
        self.retriable_statuses.push(status);
        self
    }

    /// Compute the sleep duration before retry `attempt` (0-indexed).
    /// Result is capped at `max_delay`. If jitter is on, adds up to 25% extra.
    pub(crate) fn delay_for(&self, attempt: u32) -> Duration {
        let factor = 2_u32.saturating_pow(attempt);
        let raw = self.base_delay.saturating_mul(factor).min(self.max_delay);
        if self.jitter {
            use rand::Rng;
            let extra_frac = rand::rng().random_range(0.0_f64..0.25);
            let extra = Duration::from_secs_f64(raw.as_secs_f64() * extra_frac);
            (raw + extra).min(self.max_delay)
        } else {
            raw
        }
    }

    /// Return `true` if `err` should trigger a retry under this policy.
    pub(crate) fn is_retriable(&self, err: &KumoError) -> bool {
        match err {
            KumoError::HttpStatus { status, .. } => {
                if self.retriable_statuses.is_empty() {
                    true
                } else {
                    self.retriable_statuses.contains(status)
                }
            }
            KumoError::Fetch(_) => self.retriable_statuses.is_empty(),
            // Never retry parse, store, domain, depth, llm, or browser errors.
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_doubles_each_attempt() {
        let p = RetryPolicy::new(5)
            .base_delay(Duration::from_millis(100))
            .jitter(false);
        assert_eq!(p.delay_for(0), Duration::from_millis(100));
        assert_eq!(p.delay_for(1), Duration::from_millis(200));
        assert_eq!(p.delay_for(2), Duration::from_millis(400));
    }

    #[test]
    fn delay_caps_at_max() {
        let p = RetryPolicy::new(10)
            .base_delay(Duration::from_millis(500))
            .max_delay(Duration::from_secs(5))
            .jitter(false);
        assert_eq!(p.delay_for(10), Duration::from_secs(5));
    }

    #[test]
    fn jitter_within_bounds() {
        let p = RetryPolicy::new(3)
            .base_delay(Duration::from_millis(1000))
            .max_delay(Duration::from_secs(60))
            .jitter(true);
        for _ in 0..20 {
            let d = p.delay_for(0);
            assert!(
                d >= Duration::from_millis(1000),
                "jitter must not reduce delay"
            );
            assert!(d <= Duration::from_millis(1250), "jitter exceeds 25%");
        }
    }

    #[test]
    fn is_retriable_http_status_no_filter() {
        let p = RetryPolicy::new(3);
        assert!(p.is_retriable(&KumoError::HttpStatus {
            status: 503,
            url: "u".into()
        }));
        assert!(p.is_retriable(&KumoError::HttpStatus {
            status: 429,
            url: "u".into()
        }));
    }

    #[test]
    fn is_retriable_with_status_filter() {
        let p = RetryPolicy::new(3).on_status(429);
        assert!(p.is_retriable(&KumoError::HttpStatus {
            status: 429,
            url: "u".into()
        }));
        assert!(!p.is_retriable(&KumoError::HttpStatus {
            status: 503,
            url: "u".into()
        }));
    }

    #[test]
    fn non_http_errors_never_retriable() {
        let p = RetryPolicy::new(3);
        assert!(!p.is_retriable(&KumoError::InvalidUrl("bad".into())));
        assert!(!p.is_retriable(&KumoError::parse_msg("parse fail")));
    }

    #[test]
    fn status_filter_does_not_retry_fetch_errors() {
        let p = RetryPolicy::new(3).on_status(503);
        // When a status filter is set, bare Fetch errors are not retried.
        assert!(!p.is_retriable(&KumoError::InvalidUrl("x".into())));
    }
}
