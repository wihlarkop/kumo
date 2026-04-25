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

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::USER_AGENT;

    fn make_request() -> Request {
        Request::new("https://example.com", 0)
    }

    #[tokio::test]
    async fn round_robin_cycles_in_order() {
        let rotator = UserAgentRotator::new(vec!["ua-a", "ua-b", "ua-c"]);
        let mut req = make_request();
        rotator.before_request(&mut req).await.unwrap();
        assert_eq!(req.headers[USER_AGENT], "ua-a");
        rotator.before_request(&mut req).await.unwrap();
        assert_eq!(req.headers[USER_AGENT], "ua-b");
        rotator.before_request(&mut req).await.unwrap();
        assert_eq!(req.headers[USER_AGENT], "ua-c");
        rotator.before_request(&mut req).await.unwrap();
        assert_eq!(req.headers[USER_AGENT], "ua-a");
    }

    #[tokio::test]
    async fn random_picks_from_set() {
        let agents = vec!["ua-x", "ua-y", "ua-z"];
        let rotator = UserAgentRotator::random(agents.clone());
        for _ in 0..20 {
            let mut req = make_request();
            rotator.before_request(&mut req).await.unwrap();
            let picked = req.headers[USER_AGENT].to_str().unwrap().to_string();
            assert!(agents.contains(&picked.as_str()), "unexpected UA: {picked}");
        }
    }

    #[tokio::test]
    async fn common_browsers_sets_header() {
        let rotator = UserAgentRotator::common_browsers();
        let mut req = make_request();
        rotator.before_request(&mut req).await.unwrap();
        let ua = req.headers[USER_AGENT].to_str().unwrap();
        assert!(ua.contains("Mozilla"), "expected browser UA, got: {ua}");
    }

    #[tokio::test]
    async fn empty_list_does_not_set_header() {
        let rotator = UserAgentRotator::new(Vec::<String>::new());
        let mut req = make_request();
        rotator.before_request(&mut req).await.unwrap();
        assert!(!req.headers.contains_key(USER_AGENT));
    }
}
