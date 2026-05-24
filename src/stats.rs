use std::{collections::BTreeMap, time::Duration};

use crate::error::KumoErrorKind;

/// Per-domain crawl counters collected while the engine runs.
#[derive(Debug, Default, Clone)]
pub struct DomainStats {
    pub scheduled: u64,
    pub deduped: u64,
    pub completed: u64,
    pub failed: u64,
    pub error_kinds: BTreeMap<String, u64>,
    pub retries: u64,
    pub retry_exhausted: u64,
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
    pub error_kinds: BTreeMap<String, u64>,
    pub scheduled: u64,
    pub deduped: u64,
    pub retries: u64,
    pub retry_exhausted: u64,
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

    pub fn record_error_kind(&mut self, domain: &str, kind: KumoErrorKind) {
        self.record_error(domain);
        let label = kind.as_str().to_string();
        *self.error_kinds.entry(label.clone()).or_insert(0) += 1;
        *self
            .domain_mut(domain)
            .error_kinds
            .entry(label)
            .or_insert(0) += 1;
    }

    pub fn record_retry(&mut self, domain: &str) {
        self.retries += 1;
        self.domain_mut(domain).retries += 1;
    }

    pub fn record_retry_exhausted(&mut self, domain: &str) {
        self.retry_exhausted += 1;
        self.domain_mut(domain).retry_exhausted += 1;
    }

    pub fn record_robots_blocked(&mut self, domain: &str) {
        self.robots_blocked += 1;
        self.domain_mut(domain).robots_blocked += 1;
    }

    fn domain_mut(&mut self, domain: &str) -> &mut DomainStats {
        self.domains.entry(domain.to_string()).or_default()
    }
}

impl StopReason {
    /// Stable snake_case label for reports, logs, and metrics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FrontierExhausted => "frontier_exhausted",
            Self::Interrupted => "interrupted",
            Self::MaxPages => "max_pages",
            Self::MaxItems => "max_items",
            Self::MaxDuration => "max_duration",
            Self::MaxErrors => "max_errors",
        }
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
    pub error_kinds: BTreeMap<String, u64>,
    pub scheduled: u64,
    pub deduped: u64,
    pub retries: u64,
    pub retry_exhausted: u64,
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
            error_kinds: stats.error_kinds,
            scheduled: stats.scheduled,
            deduped: stats.deduped,
            retries: stats.retries,
            retry_exhausted: stats.retry_exhausted,
            robots_blocked: stats.robots_blocked,
            domains: stats.domains,
            stop_reason: stats.stop_reason,
        }
    }
}

impl CrawlReport {
    /// Convert the report to a stable JSON value.
    ///
    /// Durations are exported as `duration_ms` and `duration_secs` so consumers
    /// do not need to know Rust's `Duration` representation.
    pub fn to_json_value(&self) -> serde_json::Value {
        let domains = self
            .domains
            .iter()
            .map(|(domain, stats)| {
                (
                    domain.clone(),
                    serde_json::json!({
                        "scheduled": stats.scheduled,
                        "deduped": stats.deduped,
                        "completed": stats.completed,
                        "failed": stats.failed,
                        "error_kinds": stats.error_kinds,
                        "retries": stats.retries,
                        "retry_exhausted": stats.retry_exhausted,
                        "robots_blocked": stats.robots_blocked,
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();

        serde_json::json!({
            "pages_crawled": self.pages_crawled,
            "items_scraped": self.items_scraped,
            "errors": self.errors,
            "duration_ms": self.duration.as_millis(),
            "duration_secs": self.duration.as_secs_f64(),
            "bytes_downloaded": self.bytes_downloaded,
            "interrupted": self.interrupted,
            "error_kinds": self.error_kinds,
            "scheduled": self.scheduled,
            "deduped": self.deduped,
            "retries": self.retries,
            "retry_exhausted": self.retry_exhausted,
            "robots_blocked": self.robots_blocked,
            "domains": domains,
            "stop_reason": self.stop_reason.map(StopReason::as_str),
        })
    }

    /// Serialize the report as compact JSON.
    pub fn to_json_string(&self) -> String {
        self.to_json_value().to_string()
    }

    /// Serialize the report as pretty-printed JSON.
    pub fn to_json_string_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.to_json_value())
            .expect("CrawlReport JSON value should always serialize")
    }
}

pub(crate) fn domain_key(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_else(|| "<unknown>".to_string())
}
