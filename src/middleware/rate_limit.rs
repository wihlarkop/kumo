use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorLimiter,
};
use std::num::NonZeroU32;
use crate::error::KumoError;
use super::{Middleware, Request};

/// Token-bucket rate limiter middleware.
///
/// Ensures at most `requests_per_second` requests are sent globally across
/// all concurrent tasks. Uses the `governor` crate for a correct implementation.
///
/// # Example
/// ```rust,ignore
/// CrawlEngine::new()
///     .middleware(RateLimiter::per_second(5.0))
/// ```
pub struct RateLimiter {
    inner: GovernorLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>,
}

impl RateLimiter {
    /// Create a global rate limiter allowing at most `rps` requests per second.
    pub fn per_second(rps: f64) -> Self {
        let rate = NonZeroU32::new((rps.ceil() as u32).max(1)).unwrap();
        let quota = Quota::per_second(rate);
        Self {
            inner: GovernorLimiter::direct(quota),
        }
    }
}

#[async_trait::async_trait]
impl Middleware for RateLimiter {
    /// Blocks until a rate-limit token is available, then proceeds.
    async fn before_request(&self, _request: &mut Request) -> Result<(), KumoError> {
        self.inner.until_ready().await;
        Ok(())
    }
}
