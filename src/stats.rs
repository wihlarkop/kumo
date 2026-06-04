use std::{collections::BTreeMap, ops::AddAssign, time::Duration};

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

/// Cumulative wall-clock time spent in major successful request phases.
///
/// These timings are intended for bottleneck diagnosis. They are accumulated
/// across successful request tasks, so concurrent phase totals can be larger
/// than the crawl's elapsed wall-clock duration.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CrawlTimingStats {
    pub middleware_request: Duration,
    pub fetch: Duration,
    pub middleware_response: Duration,
    pub parse: Duration,
    pub pipeline: Duration,
    pub store: Duration,
}

impl CrawlTimingStats {
    pub(crate) fn to_json_value(self) -> serde_json::Value {
        serde_json::json!({
            "middleware_request_ms": self.middleware_request.as_millis(),
            "middleware_request_secs": self.middleware_request.as_secs_f64(),
            "fetch_ms": self.fetch.as_millis(),
            "fetch_secs": self.fetch.as_secs_f64(),
            "middleware_response_ms": self.middleware_response.as_millis(),
            "middleware_response_secs": self.middleware_response.as_secs_f64(),
            "parse_ms": self.parse.as_millis(),
            "parse_secs": self.parse.as_secs_f64(),
            "pipeline_ms": self.pipeline.as_millis(),
            "pipeline_secs": self.pipeline.as_secs_f64(),
            "store_ms": self.store.as_millis(),
            "store_secs": self.store.as_secs_f64(),
        })
    }
}

impl AddAssign for CrawlTimingStats {
    fn add_assign(&mut self, rhs: Self) {
        self.middleware_request += rhs.middleware_request;
        self.fetch += rhs.fetch;
        self.middleware_response += rhs.middleware_response;
        self.parse += rhs.parse;
        self.pipeline += rhs.pipeline;
        self.store += rhs.store;
    }
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
    pub timings: CrawlTimingStats,
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
    pub timings: CrawlTimingStats,
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
            timings: stats.timings,
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
    /// Successful pages crawled per elapsed second.
    pub fn pages_per_second(&self) -> f64 {
        per_second(self.pages_crawled, self.duration)
    }

    /// Items scraped per elapsed second.
    pub fn items_per_second(&self) -> f64 {
        per_second(self.items_scraped, self.duration)
    }

    /// Downloaded response bytes per elapsed second.
    pub fn bytes_per_second(&self) -> f64 {
        per_second(self.bytes_downloaded, self.duration)
    }

    /// Failed requests divided by completed and failed requests.
    pub fn error_rate(&self) -> f64 {
        ratio(self.errors, self.pages_crawled + self.errors)
    }

    /// Completed requests divided by completed and failed requests.
    pub fn success_rate(&self) -> f64 {
        ratio(self.pages_crawled, self.pages_crawled + self.errors)
    }

    /// Retry-exhausted requests divided by retry attempts.
    pub fn retry_exhaustion_rate(&self) -> f64 {
        ratio(self.retry_exhausted, self.retries)
    }

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
            "pages_per_second": self.pages_per_second(),
            "items_per_second": self.items_per_second(),
            "bytes_per_second": self.bytes_per_second(),
            "bytes_downloaded": self.bytes_downloaded,
            "timings": self.timings.to_json_value(),
            "interrupted": self.interrupted,
            "error_kinds": self.error_kinds,
            "scheduled": self.scheduled,
            "deduped": self.deduped,
            "retries": self.retries,
            "retry_exhausted": self.retry_exhausted,
            "error_rate": self.error_rate(),
            "success_rate": self.success_rate(),
            "retry_exhaustion_rate": self.retry_exhaustion_rate(),
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

fn per_second(count: u64, duration: Duration) -> f64 {
    let secs = duration.as_secs_f64();
    if secs > 0.0 { count as f64 / secs } else { 0.0 }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator > 0 {
        numerator as f64 / denominator as f64
    } else {
        0.0
    }
}

pub(crate) fn domain_key(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_else(|| "<unknown>".to_string())
}
