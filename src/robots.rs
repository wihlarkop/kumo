use std::{collections::HashMap, sync::Arc};
use texting_robots::Robot;
use tokio::sync::Mutex;

/// Fetches and caches robots.txt for each domain encountered during a crawl.
///
/// Parsed `Robot` objects are stored in memory keyed by `scheme://host`.
/// A failed or missing robots.txt is treated as allowing all paths.
pub struct RobotsCache {
    cache: Mutex<HashMap<String, Option<Arc<String>>>>,
    user_agent: String,
}

impl RobotsCache {
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            user_agent: user_agent.into(),
        }
    }

    /// Check whether `url` is allowed for this cache's user agent.
    ///
    /// Fetches robots.txt on the first request to a domain, then reuses the cache.
    /// Returns `true` (allowed) if fetching robots.txt fails or it is absent.
    pub async fn is_allowed(&self, client: &reqwest::Client, url: &str) -> bool {
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(_) => return true,
        };

        let origin = format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or(""));

        // Return cached result if available.
        if let Some(entry) = self.cache.lock().await.get(&origin).cloned() {
            return Self::robot_allows(&self.user_agent, entry.as_deref().map(|s| s.as_str()), url);
        }

        // Fetch robots.txt.
        let robots_url = format!("{}/robots.txt", origin);
        let txt = client
            .get(&robots_url)
            .send()
            .await
            .ok()
            .filter(|r| r.status().is_success())
            .map(|r| async move { r.text().await.ok() });

        let txt: Option<String> = match txt {
            Some(fut) => fut.await,
            None => None,
        };

        let entry: Option<Arc<String>> = txt.map(Arc::new);
        let allowed =
            Self::robot_allows(&self.user_agent, entry.as_deref().map(|s| s.as_str()), url);
        self.cache.lock().await.insert(origin, entry);
        allowed
    }

    fn robot_allows(user_agent: &str, txt: Option<&str>, url: &str) -> bool {
        match txt {
            None => true,
            Some(content) => Robot::new(user_agent, content.as_bytes())
                .map(|r| r.allowed(url))
                .unwrap_or(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // robot_allows is a private static method — test it directly.

    #[test]
    fn no_robots_txt_allows_all() {
        assert!(RobotsCache::robot_allows("kumo", None, "https://example.com/anything"));
    }

    #[test]
    fn disallow_all_blocks_all_paths() {
        let txt = "User-agent: *\nDisallow: /\n";
        assert!(!RobotsCache::robot_allows("kumo", Some(txt), "https://example.com/page"));
    }

    #[test]
    fn disallow_specific_path_blocks_that_path() {
        let txt = "User-agent: *\nDisallow: /private/\n";
        assert!(!RobotsCache::robot_allows("kumo", Some(txt), "https://example.com/private/secret"));
    }

    #[test]
    fn disallow_specific_path_allows_other_paths() {
        let txt = "User-agent: *\nDisallow: /private/\n";
        assert!(RobotsCache::robot_allows("kumo", Some(txt), "https://example.com/public/page"));
    }

    #[test]
    fn allow_all_explicit() {
        let txt = "User-agent: *\nDisallow:\n";
        assert!(RobotsCache::robot_allows("kumo", Some(txt), "https://example.com/anything"));
    }

    #[test]
    fn malformed_robots_txt_allows_all() {
        let txt = "this is not a valid robots.txt !!!@#$";
        // parse error → default allow
        assert!(RobotsCache::robot_allows("kumo", Some(txt), "https://example.com/page"));
    }

    #[test]
    fn specific_user_agent_disallow_does_not_block_other_agents() {
        let txt = "User-agent: badbot\nDisallow: /\n";
        assert!(RobotsCache::robot_allows("kumo", Some(txt), "https://example.com/page"));
    }
}
