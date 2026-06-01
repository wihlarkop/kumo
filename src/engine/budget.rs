use std::time::{Duration, Instant};

use crate::stats::{CrawlStats, StopReason};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CrawlBudgets {
    pub(super) max_pages: Option<u64>,
    pub(super) max_items: Option<u64>,
    pub(super) max_duration: Option<Duration>,
    pub(super) max_errors: Option<u64>,
}

impl CrawlBudgets {
    pub(super) fn stop_reason(&self, stats: &CrawlStats, start: Instant) -> Option<StopReason> {
        if stats.interrupted {
            return Some(StopReason::Interrupted);
        }
        if self
            .max_duration
            .is_some_and(|max_duration| start.elapsed() >= max_duration)
        {
            return Some(StopReason::MaxDuration);
        }
        if self
            .max_errors
            .is_some_and(|max_errors| stats.errors >= max_errors)
        {
            return Some(StopReason::MaxErrors);
        }
        if self
            .max_pages
            .is_some_and(|max_pages| stats.pages_crawled >= max_pages)
        {
            return Some(StopReason::MaxPages);
        }
        if self
            .max_items
            .is_some_and(|max_items| stats.items_scraped >= max_items)
        {
            return Some(StopReason::MaxItems);
        }
        None
    }

    pub(super) fn mark_if_reached(&self, stats: &mut CrawlStats, start: Instant) -> bool {
        if stats.stop_reason.is_none() {
            stats.stop_reason = self.stop_reason(stats, start);
        }
        stats.stop_reason.is_some()
    }

    pub(super) fn remaining_duration(&self, start: Instant) -> Option<Duration> {
        self.max_duration
            .map(|max_duration| max_duration.saturating_sub(start.elapsed()))
    }
}
