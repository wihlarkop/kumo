use std::time::Duration;

use tokio::sync::broadcast;

use crate::{
    error::KumoErrorKind,
    stats::{CrawlReport, StopReason},
};

/// Why a request was skipped before fetching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSkipReason {
    RobotsTxt,
    Duplicate,
    DepthLimit,
    DomainDenied,
}

impl RequestSkipReason {
    /// Stable snake_case label for logs, reports, and event consumers.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RobotsTxt => "robots_txt",
            Self::Duplicate => "duplicate",
            Self::DepthLimit => "depth_limit",
            Self::DomainDenied => "domain_denied",
        }
    }
}

/// Why an item was dropped before storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemDropReason {
    PipelineFiltered,
    PipelineError,
}

impl ItemDropReason {
    /// Stable snake_case label for logs, reports, and event consumers.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PipelineFiltered => "pipeline_filtered",
            Self::PipelineError => "pipeline_error",
        }
    }
}

/// Typed crawl lifecycle event emitted by [`crate::engine::CrawlEngine`].
///
/// Event delivery is best-effort. If there are no receivers or a receiver lags,
/// the crawl continues normally.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum CrawlEvent {
    CrawlStarted {
        spider: String,
        spider_index: Option<usize>,
        start_urls: usize,
    },
    RequestScheduled {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        domain: String,
        depth: usize,
    },
    RequestSkipped {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        domain: String,
        depth: usize,
        reason: RequestSkipReason,
    },
    RequestStarted {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        domain: String,
        depth: usize,
        attempt: u32,
    },
    RequestCompleted {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        domain: String,
        depth: usize,
        attempt: u32,
        status: u16,
        bytes: u64,
        items: u64,
        elapsed: Duration,
    },
    RequestRetried {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        domain: String,
        depth: usize,
        attempt: u32,
        max_attempts: u32,
        delay: Duration,
        error_kind: KumoErrorKind,
    },
    RequestFailed {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        domain: String,
        depth: usize,
        attempt: u32,
        error_kind: KumoErrorKind,
        retry_exhausted: bool,
    },
    TaskPanicked {
        spider: String,
        spider_index: Option<usize>,
        url: Option<String>,
        domain: Option<String>,
        depth: Option<usize>,
    },
    ItemScraped {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        depth: usize,
    },
    ItemDropped {
        spider: String,
        spider_index: Option<usize>,
        url: String,
        depth: usize,
        reason: ItemDropReason,
        error_kind: Option<KumoErrorKind>,
    },
    CrawlFinished {
        spider: String,
        spider_index: Option<usize>,
        report: CrawlReport,
        stop_reason: Option<StopReason>,
    },
}

impl CrawlEvent {
    /// Stable snake_case label for dashboards, metrics, and event logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::CrawlStarted { .. } => "crawl_started",
            Self::RequestScheduled { .. } => "request_scheduled",
            Self::RequestSkipped { .. } => "request_skipped",
            Self::RequestStarted { .. } => "request_started",
            Self::RequestCompleted { .. } => "request_completed",
            Self::RequestRetried { .. } => "request_retried",
            Self::RequestFailed { .. } => "request_failed",
            Self::TaskPanicked { .. } => "task_panicked",
            Self::ItemScraped { .. } => "item_scraped",
            Self::ItemDropped { .. } => "item_dropped",
            Self::CrawlFinished { .. } => "crawl_finished",
        }
    }
}

/// Best-effort event emitter used internally by the crawl engine.
#[derive(Debug, Clone)]
pub(crate) struct EventEmitter {
    tx: broadcast::Sender<CrawlEvent>,
}

impl EventEmitter {
    pub(crate) fn new(tx: broadcast::Sender<CrawlEvent>) -> Self {
        Self { tx }
    }

    pub(crate) fn emit(&self, event: CrawlEvent) {
        let _ = self.tx.send(event);
    }
}
