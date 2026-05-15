use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use texting_robots::Robot;
use tokio::sync::Mutex;

const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours

#[derive(Clone)]
struct CacheEntry {
    txt: Option<Arc<String>>,
    crawl_delay: Option<Duration>,
    fetched_at: Instant,
}

fn parse_crawl_delay_value(value: &str) -> Option<Duration> {
    let seconds = value.trim().parse::<f64>().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some(Duration::from_secs_f64(seconds))
}

/// Parse the best matching `Crawl-delay` value for `user_agent`.
///
/// Exact user-agent groups take precedence over wildcard (`*`) groups.
pub fn parse_crawl_delay_for_agent(txt: &str, user_agent: &str) -> Option<Duration> {
    fn finish_group(
        agents: &[String],
        delay: Option<Duration>,
        wanted: &str,
        best: &mut Option<(u8, Duration)>,
    ) {
        let Some(delay) = delay else {
            return;
        };

        let score = agents.iter().fold(0, |score, agent| {
            if agent == wanted {
                score.max(2)
            } else if agent == "*" {
                score.max(1)
            } else {
                score
            }
        });

        if score > best.map_or(0, |(best_score, _)| best_score) {
            *best = Some((score, delay));
        }
    }

    let wanted = user_agent.to_ascii_lowercase();
    let mut best = None;
    let mut agents = Vec::new();
    let mut delay = None;
    let mut saw_directive = false;

    for raw_line in txt.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            finish_group(&agents, delay, &wanted, &mut best);
            agents.clear();
            delay = None;
            saw_directive = false;
            continue;
        }

        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();

        if name == "user-agent" {
            if saw_directive {
                finish_group(&agents, delay, &wanted, &mut best);
                agents.clear();
                delay = None;
                saw_directive = false;
            }
            agents.push(value.to_ascii_lowercase());
        } else {
            saw_directive = true;
            if name == "crawl-delay" && delay.is_none() {
                delay = parse_crawl_delay_value(value);
            }
        }
    }

    finish_group(&agents, delay, &wanted, &mut best);
    best.map(|(_, delay)| delay)
}

/// Fetches and caches robots.txt for each domain encountered during a crawl.
///
/// Entries expire after a configurable TTL (default: 24 hours), matching the
/// de-facto standard used by most crawlers. Expired entries are re-fetched
/// transparently on the next request to that domain.
///
/// A failed or missing robots.txt is treated as allowing all paths.
pub struct RobotsCache {
    cache: Mutex<HashMap<String, CacheEntry>>,
    user_agent: String,
    ttl: Duration,
}

impl RobotsCache {
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self::with_ttl(user_agent, DEFAULT_TTL)
    }

    pub fn with_ttl(user_agent: impl Into<String>, ttl: Duration) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            user_agent: user_agent.into(),
            ttl,
        }
    }

    /// Check whether `url` is allowed for this cache's user agent.
    ///
    /// Fetches robots.txt on the first request to a domain, then reuses the
    /// cache until the TTL expires. Returns `true` (allowed) if fetching
    /// robots.txt fails or it is absent.
    pub async fn is_allowed(&self, client: &reqwest::Client, url: &str) -> bool {
        let Some(entry) = self.entry_for_url(client, url).await else {
            return true;
        };
        Self::robot_allows(
            &self.user_agent,
            entry.txt.as_deref().map(|s| s.as_str()),
            url,
        )
    }

    /// Return this URL's robots.txt `Crawl-delay`, if present for the cache user agent.
    pub async fn crawl_delay(&self, client: &reqwest::Client, url: &str) -> Option<Duration> {
        self.entry_for_url(client, url)
            .await
            .and_then(|entry| entry.crawl_delay)
    }

    async fn entry_for_url(&self, client: &reqwest::Client, url: &str) -> Option<CacheEntry> {
        let parsed = match url::Url::parse(url) {
            Ok(parsed) => parsed,
            Err(_) => return None,
        };

        let origin = format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or(""));

        // Return cached result if still fresh.
        {
            let cache = self.cache.lock().await;
            if let Some(entry) = cache.get(&origin)
                && entry.fetched_at.elapsed() < self.ttl
            {
                return Some(entry.clone());
            }
        }

        // Fetch (or re-fetch after TTL expiry).
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

        let txt = txt.map(Arc::new);
        let crawl_delay = txt
            .as_deref()
            .and_then(|txt| parse_crawl_delay_for_agent(txt, &self.user_agent));
        let entry = CacheEntry {
            txt,
            crawl_delay,
            fetched_at: Instant::now(),
        };
        self.cache.lock().await.insert(origin, entry.clone());
        Some(entry)
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
