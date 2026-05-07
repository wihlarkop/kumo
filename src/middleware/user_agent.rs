use async_trait::async_trait;
use reqwest::header::{HeaderValue, USER_AGENT};

use crate::error::KumoError;

use super::{Middleware, Request, RotationStrategy};

/// Common desktop browser User-Agent strings (Chrome, Firefox, Safari across Win/Mac/Linux).
const COMMON_BROWSERS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:125.0) Gecko/20100101 Firefox/125.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.4; rv:125.0) Gecko/20100101 Firefox/125.0",
    "Mozilla/5.0 (X11; Linux x86_64; rv:125.0) Gecko/20100101 Firefox/125.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_4_1) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4.1 Safari/605.1.15",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0",
];

/// Middleware that rotates the `User-Agent` header on every request.
///
/// # Examples
/// ```rust,ignore
/// UserAgentRotator::common_browsers()          // preset desktop browsers
/// UserAgentRotator::new(vec!["ua1", "ua2"])    // round-robin
/// UserAgentRotator::random(vec!["ua1", "ua2"]) // pseudo-random pick
/// ```
pub struct UserAgentRotator {
    agents: Vec<String>,
    strategy: RotationStrategy,
}

impl UserAgentRotator {
    /// Rotate through `agents` in order (round-robin).
    pub fn new(agents: Vec<impl Into<String>>) -> Self {
        Self {
            agents: agents.into_iter().map(Into::into).collect(),
            strategy: RotationStrategy::round_robin(),
        }
    }

    /// Pick randomly from `agents` on each request.
    pub fn random(agents: Vec<impl Into<String>>) -> Self {
        Self {
            agents: agents.into_iter().map(Into::into).collect(),
            strategy: RotationStrategy::random(),
        }
    }

    /// Use a built-in preset of common desktop browser User-Agent strings.
    pub fn common_browsers() -> Self {
        Self::new(COMMON_BROWSERS.to_vec())
    }

    fn pick(&self) -> Option<&str> {
        if self.agents.is_empty() {
            return None;
        }
        Some(&self.agents[self.strategy.pick_index(self.agents.len())])
    }
}

impl Default for UserAgentRotator {
    fn default() -> Self {
        Self::common_browsers()
    }
}

#[async_trait]
impl Middleware for UserAgentRotator {
    async fn before_request(&self, request: &mut Request) -> Result<(), KumoError> {
        if let Some(ua) = self.pick()
            && let Ok(value) = HeaderValue::from_str(ua)
        {
            request.headers.insert(USER_AGENT, value);
        }
        Ok(())
    }
}
