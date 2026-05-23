use std::{collections::BTreeMap, time::Duration};

/// Per-domain crawl counters collected while the engine runs.
#[derive(Debug, Default, Clone)]
pub struct DomainStats {
    pub scheduled: u64,
    pub deduped: u64,
    pub completed: u64,
    pub failed: u64,
    pub retries: u64,
    pub robots_blocked: u64,
}

/// Why a crawl stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The frontier ran out of eligible requests.
    FrontierExhausted,
    /// The crawl was interrupted, for example by Ctrl+C or stream cancellation.
    Interrupted,
    /// The configured page budget was reached.
    MaxPages,
    /// The configured item budget was reached.
    MaxItems,
    /// The configured duration budget was reached.
    MaxDuration,
    /// The configured error budget was reached.
    MaxErrors,
}

/// Statistics returned by `CrawlEngine::run` after the crawl finishes.
#[derive(Debug, Default, Clone)]
pub struct CrawlStats {
    pub pages_crawled: u64,
    pub items_scraped: u64,
    pub errors: u64,
    pub duration: Duration,
    pub bytes_downloaded: u64,
    /// `true` when the crawl was stopped early by Ctrl+C.
    pub interrupted: bool,
    pub scheduled: u64,
    pub deduped: u64,
    pub retries: u64,
    pub robots_blocked: u64,
    pub domains: BTreeMap<String, DomainStats>,
    pub stop_reason: Option<StopReason>,
}

impl CrawlStats {
    pub fn record_scheduled(&mut self, domain: &str) {
        self.scheduled += 1;
        self.domain_mut(domain).scheduled += 1;
    }

    pub fn record_deduped(&mut self, domain: &str) {
        self.deduped += 1;
        self.domain_mut(domain).deduped += 1;
    }

    pub fn record_completed(&mut self, domain: &str) {
        self.domain_mut(domain).completed += 1;
    }

    pub fn record_failed(&mut self, domain: &str) {
        self.domain_mut(domain).failed += 1;
    }

    pub fn record_error(&mut self, domain: &str) {
        self.errors += 1;
        self.record_failed(domain);
    }

    pub fn record_retry(&mut self, domain: &str) {
        self.retries += 1;
        self.domain_mut(domain).retries += 1;
    }

    pub fn record_robots_blocked(&mut self, domain: &str) {
        self.robots_blocked += 1;
        self.domain_mut(domain).robots_blocked += 1;
    }

    fn domain_mut(&mut self, domain: &str) -> &mut DomainStats {
        self.domains.entry(domain.to_string()).or_default()
    }
}

/// Final crawl report. This is a stable, cloneable snapshot of [`CrawlStats`].
#[derive(Debug, Clone)]
pub struct CrawlReport {
    pub pages_crawled: u64,
    pub items_scraped: u64,
    pub errors: u64,
    pub duration: Duration,
    pub bytes_downloaded: u64,
    pub interrupted: bool,
    pub scheduled: u64,
    pub deduped: u64,
    pub retries: u64,
    pub robots_blocked: u64,
    pub domains: BTreeMap<String, DomainStats>,
    pub stop_reason: Option<StopReason>,
}

impl From<CrawlStats> for CrawlReport {
    fn from(stats: CrawlStats) -> Self {
        Self {
            pages_crawled: stats.pages_crawled,
            items_scraped: stats.items_scraped,
            errors: stats.errors,
            duration: stats.duration,
            bytes_downloaded: stats.bytes_downloaded,
            interrupted: stats.interrupted,
            scheduled: stats.scheduled,
            deduped: stats.deduped,
            retries: stats.retries,
            robots_blocked: stats.robots_blocked,
            domains: stats.domains,
            stop_reason: stats.stop_reason,
        }
    }
}

pub(crate) fn domain_key(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_else(|| "<unknown>".to_string())
}
