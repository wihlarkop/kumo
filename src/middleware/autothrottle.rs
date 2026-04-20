use super::{Middleware, Request};
use crate::{error::KumoError, extract::Response};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

struct ThrottleState {
    current_delay: Duration,
    ewma_latency_secs: f64,
}

/// Adaptive rate-throttle middleware.
///
/// Adjusts the inter-request delay in real time based on observed response
/// latency (EWMA-smoothed) and error status codes. Faster server → shorter
/// delay. Slow server or 429/503 → back off automatically.
///
/// # Example
/// ```rust,ignore
/// CrawlEngine::builder()
///     .middleware(AutoThrottle::new())
///     .run(MySpider)
///     .await?;
/// ```
pub struct AutoThrottle {
    target_concurrency: f64,
    min_delay: Duration,
    max_delay: Duration,
    backoff_statuses: Vec<u16>,
    state: Arc<Mutex<ThrottleState>>,
}

impl AutoThrottle {
    /// Create with defaults: start_delay=500ms, min=100ms, max=60s, target_concurrency=1.0.
    pub fn new() -> Self {
        let start_delay = Duration::from_millis(500);
        Self {
            target_concurrency: 1.0,
            min_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            backoff_statuses: vec![429, 503],
            state: Arc::new(Mutex::new(ThrottleState {
                current_delay: start_delay,
                ewma_latency_secs: start_delay.as_secs_f64(),
            })),
        }
    }

    /// Aim for this many concurrent requests to the target (default: 1.0).
    ///
    /// Higher values allow shorter delays when latency is high.
    pub fn target_concurrency(mut self, n: f64) -> Self {
        self.target_concurrency = n.max(0.1);
        self
    }

    /// Initial delay before any responses are observed (default: 500ms).
    pub fn start_delay(self, d: Duration) -> Self {
        let mut st = self.state.lock().unwrap();
        st.current_delay = d;
        st.ewma_latency_secs = d.as_secs_f64();
        drop(st);
        self
    }

    /// Floor for the adaptive delay (default: 100ms).
    pub fn min_delay(mut self, d: Duration) -> Self {
        self.min_delay = d;
        self
    }

    /// Ceiling for the adaptive delay (default: 60s).
    pub fn max_delay(mut self, d: Duration) -> Self {
        self.max_delay = d;
        self
    }

    /// HTTP status codes that trigger an immediate delay doubling (default: [429, 503]).
    pub fn backoff_statuses(mut self, codes: Vec<u16>) -> Self {
        self.backoff_statuses = codes;
        self
    }
}

impl Default for AutoThrottle {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Middleware for AutoThrottle {
    async fn before_request(&self, _request: &mut Request) -> Result<(), KumoError> {
        let delay = self.state.lock().unwrap().current_delay;
        tokio::time::sleep(delay).await;
        Ok(())
    }

    async fn after_response(&self, response: &mut Response) -> Result<(), KumoError> {
        let latency = response.elapsed().as_secs_f64();
        let mut st = self.state.lock().unwrap();

        st.ewma_latency_secs = 0.3 * latency + 0.7 * st.ewma_latency_secs;

        let new_delay = if self.backoff_statuses.contains(&response.status()) {
            (st.current_delay * 2).min(self.max_delay)
        } else {
            let target_secs = st.ewma_latency_secs / self.target_concurrency;
            let blended = (st.current_delay.as_secs_f64() + target_secs) / 2.0;
            Duration::from_secs_f64(blended)
        };

        st.current_delay = new_delay.clamp(self.min_delay, self.max_delay);
        tracing::debug!(
            delay_ms = st.current_delay.as_millis(),
            ewma_latency_ms = (st.ewma_latency_secs * 1000.0) as u64,
            status = response.status(),
            "autothrottle adjusted delay"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Response;

    fn make_response(status: u16, elapsed_ms: u64) -> Response {
        Response::from_parts("http://example.com", status, "")
            .with_elapsed(Duration::from_millis(elapsed_ms))
    }

    #[tokio::test]
    async fn delay_adapts_toward_latency() {
        let throttle = AutoThrottle::new()
            .start_delay(Duration::from_millis(500))
            .min_delay(Duration::from_millis(10))
            .max_delay(Duration::from_secs(60));

        // Feed fast responses: 100ms latency
        for _ in 0..10 {
            let mut resp = make_response(200, 100);
            throttle.after_response(&mut resp).await.unwrap();
        }

        let delay = throttle.state.lock().unwrap().current_delay;
        // Should have converged below start_delay (500ms) toward ~100ms
        assert!(
            delay < Duration::from_millis(500),
            "delay should decrease for fast responses"
        );
        assert!(
            delay >= Duration::from_millis(10),
            "delay should not go below min"
        );
    }

    #[tokio::test]
    async fn delay_doubles_on_backoff_status() {
        let throttle = AutoThrottle::new()
            .start_delay(Duration::from_millis(200))
            .max_delay(Duration::from_secs(60));

        let initial = throttle.state.lock().unwrap().current_delay;
        let mut resp = make_response(429, 50);
        throttle.after_response(&mut resp).await.unwrap();

        let after = throttle.state.lock().unwrap().current_delay;
        assert_eq!(after, (initial * 2).min(Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn delay_is_clamped_to_max() {
        let throttle = AutoThrottle::new()
            .start_delay(Duration::from_secs(30))
            .max_delay(Duration::from_secs(60));

        // Repeated 429s should not exceed max
        for _ in 0..10 {
            let mut resp = make_response(429, 100);
            throttle.after_response(&mut resp).await.unwrap();
        }

        let delay = throttle.state.lock().unwrap().current_delay;
        assert!(
            delay <= Duration::from_secs(60),
            "delay must not exceed max_delay"
        );
    }

    #[tokio::test]
    async fn delay_is_clamped_to_min() {
        let throttle = AutoThrottle::new()
            .start_delay(Duration::from_millis(200))
            .min_delay(Duration::from_millis(50));

        // Feed very fast responses — delay should floor at min
        for _ in 0..50 {
            let mut resp = make_response(200, 1);
            throttle.after_response(&mut resp).await.unwrap();
        }

        let delay = throttle.state.lock().unwrap().current_delay;
        assert!(
            delay >= Duration::from_millis(50),
            "delay must not go below min_delay"
        );
    }
}
