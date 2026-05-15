use std::time::Duration;

use crate::error::KumoError;

const JITTER_FRACTION: f64 = 0.25;

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
    /// `max_attempts` is the number of *retries* - total fetch calls = `max_attempts + 1`.
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

    /// Add up to 25% random jitter to each delay so concurrent retries do not thundering-herd.
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

    /// Number of retries allowed by this policy.
    ///
    /// Total fetch calls can be up to `max_attempts + 1`, because the first
    /// fetch is not counted as a retry.
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Return the configured base delay before exponential backoff.
    pub fn base_delay_value(&self) -> Duration {
        self.base_delay
    }

    /// Return the maximum retry delay cap.
    pub fn max_delay_value(&self) -> Duration {
        self.max_delay
    }

    /// Return whether this policy adds jitter to retry delays.
    pub fn jitter_enabled(&self) -> bool {
        self.jitter
    }

    /// Return the configured HTTP status filter.
    ///
    /// An empty slice means all `KumoError::HttpStatus` and `KumoError::Fetch`
    /// errors are retryable.
    pub fn retriable_statuses(&self) -> &[u16] {
        &self.retriable_statuses
    }

    /// Compute the deterministic exponential-backoff delay before retry
    /// `attempt` (0-indexed), excluding random jitter.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let factor = 2_u32.saturating_pow(attempt);
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }

    /// Compute the deterministic retry delay for `attempt`, preferring a
    /// server-provided delay hint when one is available.
    ///
    /// The hint is capped by `max_delay`. If no hint is provided, this returns
    /// the normal exponential-backoff delay without jitter.
    pub fn delay_for_attempt_with_hint(&self, attempt: u32, hint: Option<Duration>) -> Duration {
        hint.map_or_else(
            || self.delay_for_attempt(attempt),
            |delay| delay.min(self.max_delay),
        )
    }

    /// Return the inclusive delay range that can be produced for `attempt`.
    ///
    /// When jitter is disabled, both bounds are the deterministic delay.
    /// When jitter is enabled, the upper bound includes up to 25% extra delay
    /// and is still capped by `max_delay`.
    pub fn delay_bounds_for_attempt(&self, attempt: u32) -> (Duration, Duration) {
        let base = self.delay_for_attempt(attempt);
        if !self.jitter {
            return (base, base);
        }

        let max_jitter = Duration::from_secs_f64(base.as_secs_f64() * JITTER_FRACTION);
        (base, (base + max_jitter).min(self.max_delay))
    }

    /// Return `true` if `err` should trigger a retry under this policy.
    pub fn is_retriable_error(&self, err: &KumoError) -> bool {
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

    /// Compute the sleep duration before retry `attempt` (0-indexed).
    /// Result is capped at `max_delay`. If jitter is on, adds up to 25% extra.
    pub(crate) fn delay_for(&self, attempt: u32) -> Duration {
        let raw = self.delay_for_attempt(attempt);
        if self.jitter {
            use rand::Rng;
            let extra_frac = rand::rng().random_range(0.0_f64..JITTER_FRACTION);
            let extra = Duration::from_secs_f64(raw.as_secs_f64() * extra_frac);
            (raw + extra).min(self.max_delay)
        } else {
            raw
        }
    }

    pub(crate) fn delay_for_with_hint(&self, attempt: u32, hint: Option<Duration>) -> Duration {
        hint.map_or_else(
            || self.delay_for(attempt),
            |delay| delay.min(self.max_delay),
        )
    }

    /// Return `true` if `err` should trigger a retry under this policy.
    pub(crate) fn is_retriable(&self, err: &KumoError) -> bool {
        self.is_retriable_error(err)
    }
}
