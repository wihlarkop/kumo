use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use texting_robots::Robot;

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

        let origin = format!(
            "{}://{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or("")
        );

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
        let allowed = Self::robot_allows(&self.user_agent, entry.as_deref().map(|s| s.as_str()), url);
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
