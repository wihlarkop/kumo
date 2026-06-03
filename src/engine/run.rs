use std::{collections::HashMap, sync::Arc};

use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    error::{ErrorPolicy, KumoError},
    frontier::{Frontier, memory::MemoryFrontier},
    logging::{event, target},
    middleware::Middleware,
    pipeline::Pipeline,
    request::{CrawlRequest, FrontierRequest},
    scheduler::{CrawlScheduler, SchedulerPoll},
    spider::Spider,
    stats::{CrawlStats, domain_key},
};

use super::{
    budget::CrawlBudgets,
    builder::CrawlEngine,
    erased::{ErasedSpider, SpiderErased},
    setup::{
        FetcherArgs, build_http_client, build_raw_fetcher, build_robots_cache, wrap_with_cache,
    },
    task::{
        RequestTaskOutput, TaskContext, is_cancelled, process_request_once, should_enqueue,
        skip_reason,
    },
};

async fn update_live_stats(
    metrics_interval: Option<std::time::Duration>,
    live_stats: &Arc<tokio::sync::Mutex<CrawlStats>>,
    stats: &CrawlStats,
    start: std::time::Instant,
) {
    if metrics_interval.is_some() {
        let mut snap = live_stats.lock().await;
        *snap = stats.clone();
        snap.duration = start.elapsed();
    }
}

impl CrawlEngine {
    /// Consume the engine, run the spider, and return crawl statistics.
    pub async fn run<S>(self, spider: S) -> Result<CrawlStats, KumoError>
    where
        S: Spider + 'static,
    {
        let start = std::time::Instant::now();
        let budgets = CrawlBudgets {
            max_pages: self.max_pages,
            max_items: self.max_items,
            max_duration: self.max_duration,
            max_errors: self.max_errors,
        };
        let metrics_interval = self.metrics_interval;
        let events = self.events.clone();
        let stream_cancelled = self.stream_cancelled.clone();
        let spider: Arc<dyn ErasedSpider> = Arc::new(SpiderErased(spider));
        let frontier: Arc<dyn Frontier> = self
            .frontier
            .unwrap_or_else(|| Arc::new(MemoryFrontier::new(self.max_urls)));
        let scheduler = CrawlScheduler::from_arc(frontier, self.politeness_policy)
            .with_fingerprint_policy(self.fingerprint_policy);
        let store = self
            .store
            .unwrap_or_else(|| Arc::new(crate::store::stdout::StdoutStore));
        let middleware: Arc<Vec<Arc<dyn Middleware>>> = Arc::new(self.middleware);
        let pipelines: Arc<Vec<Arc<dyn Pipeline>>> = Arc::new(self.pipelines);

        // Warn if both AutoThrottle and RateLimiter are registered â€” they compound delays.
        {
            let has_throttle = middleware
                .iter()
                .any(|mw| std::any::type_name_of_val(mw.as_ref()).contains("AutoThrottle"));
            let has_limiter = middleware
                .iter()
                .any(|mw| std::any::type_name_of_val(mw.as_ref()).contains("RateLimiter"));
            if has_throttle && has_limiter {
                tracing::warn!(
                    target: target::CRAWL,
                    event = "crawl.middleware_conflict",
                    middleware_a = "AutoThrottle",
                    middleware_b = "RateLimiter",
                    "Both AutoThrottle and RateLimiter are registered. \
                     They apply delays independently and will compound. \
                     Consider using only one."
                );
            }
        }
        let concurrency = self.concurrency;
        let retry_policy = self.retry_policy;
        let robots_cache = build_robots_cache(self.respect_robots, self.robots_ttl);
        let client =
            build_http_client(concurrency, self.request_timeout, self.http_client_builder)?;
        let fetcher = build_raw_fetcher(FetcherArgs {
            fetcher_override: self.fetcher_override,
            client: client.clone(),
            concurrency,
            #[cfg(feature = "stealth")]
            stealth_profile: self.stealth_profile,
            #[cfg(feature = "browser")]
            browser: self.browser,
        })
        .await?;
        let fetcher = wrap_with_cache(fetcher, self.cache_dir, self.cache_ttl)?;

        let mut stats = CrawlStats::default();

        spider.open().await?;

        let start_urls = spider.start_urls();
        if let Some(events) = &events {
            events.emit(crate::events::CrawlEvent::CrawlStarted {
                spider: spider.name().to_string(),
                spider_index: None,
                start_urls: start_urls.len(),
            });
        }
        info!(
            target: target::CRAWL,
            event = event::CRAWL_START,
            spider = spider.name(),
            start_urls = start_urls.len(),
            "crawl.start"
        );
        for url in start_urls {
            let domain = domain_key(&url);
            if scheduler
                .push_request(CrawlRequest::get(url.clone()), 0)
                .await
            {
                stats.record_scheduled(&domain);
                if let Some(events) = &events {
                    events.emit(crate::events::CrawlEvent::RequestScheduled {
                        spider: spider.name().to_string(),
                        spider_index: None,
                        url,
                        domain,
                        depth: 0,
                    });
                }
            } else {
                stats.record_deduped(&domain);
                if let Some(events) = &events {
                    events.emit(crate::events::CrawlEvent::RequestSkipped {
                        spider: spider.name().to_string(),
                        spider_index: None,
                        url,
                        domain,
                        depth: 0,
                        reason: crate::events::RequestSkipReason::Duplicate,
                    });
                }
            }
        }

        type TaskResult = (FrontierRequest, Result<RequestTaskOutput, KumoError>);
        let mut join_set: JoinSet<TaskResult> = JoinSet::new();
        let mut task_context = HashMap::new();

        // Spawn periodic metrics logger if configured.
        let live_stats = Arc::new(tokio::sync::Mutex::new(CrawlStats::default()));
        let _metrics_task = metrics_interval.map(|interval| {
            let live = live_stats.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(interval).await;
                    let s = live.lock().await;
                    tracing::info!(
                        target: target::CRAWL,
                        event = event::CRAWL_METRICS,
                        pages = s.pages_crawled,
                        items = s.items_scraped,
                        errors = s.errors,
                        retries = s.retries,
                        retry_exhausted = s.retry_exhausted,
                        bytes = s.bytes_downloaded,
                        elapsed_secs = s.duration.as_secs_f64(),
                        "crawl.metrics"
                    );
                }
            })
        });

        let shutdown = async {
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!(
                    target: target::CRAWL,
                    event = event::CRAWL_INTERRUPTED,
                    "crawl.interrupted"
                );
            }
            #[cfg(target_arch = "wasm32")]
            std::future::pending::<()>().await
        };
        tokio::pin!(shutdown);
        let mut shutting_down = false;

        loop {
            if is_cancelled(&stream_cancelled) {
                shutting_down = true;
                stats.interrupted = true;
                stats.stop_reason = Some(crate::stats::StopReason::Interrupted);
            }

            if !shutting_down && budgets.mark_if_reached(&mut stats, start) {
                shutting_down = true;
            }

            let mut next_scheduler_wait: Option<std::time::Duration> = None;

            if !shutting_down {
                // Fill up to the concurrency limit.
                while join_set.len() < concurrency {
                    match scheduler.poll_ready().await {
                        SchedulerPoll::Ready(queued) => {
                            let queued = *queued;
                            // Check robots.txt before dispatching.
                            if let Some(ref cache) = robots_cache
                                && !cache.is_allowed(&client, queued.request.url()).await
                            {
                                if let Some(events) = &events {
                                    let url = queued.request.url().to_string();
                                    events.emit(crate::events::CrawlEvent::RequestSkipped {
                                        spider: spider.name().to_string(),
                                        spider_index: None,
                                        domain: domain_key(&url),
                                        url,
                                        depth: queued.depth,
                                        reason: crate::events::RequestSkipReason::RobotsTxt,
                                    });
                                }
                                tracing::debug!(
                                    target: target::REQUEST,
                                    event = event::REQUEST_ROBOTS_BLOCKED,
                                    spider = spider.name(),
                                    url = %queued.request.url(),
                                    domain = %domain_key(queued.request.url()),
                                    depth = queued.depth,
                                    attempt = queued.retry_count,
                                    "request.robots_blocked"
                                );
                                stats.record_robots_blocked(&domain_key(queued.request.url()));
                                update_live_stats(metrics_interval, &live_stats, &stats, start)
                                    .await;
                                scheduler.finish(&queued).await;
                                continue;
                            }
                            if let Some(ref cache) = robots_cache
                                && let Some(delay) =
                                    cache.crawl_delay(&client, queued.request.url()).await
                            {
                                scheduler
                                    .observe_robots_crawl_delay(queued.request.url(), delay)
                                    .await;
                            }

                            let ctx = TaskContext {
                                spider: spider.clone(),
                                spider_index: None,
                                store: store.clone(),
                                middleware: middleware.clone(),
                                pipelines: pipelines.clone(),
                                fetcher: fetcher.clone(),
                                events: events.clone(),
                                stream_cancelled: stream_cancelled.clone(),
                            };

                            let task_queued = queued.clone();
                            let task_id = join_set
                                .spawn(async move {
                                    let result =
                                        process_request_once(task_queued.clone(), ctx).await;
                                    (task_queued, result)
                                })
                                .id();
                            task_context.insert(task_id, queued);
                        }
                        SchedulerPoll::Pending(wait) => {
                            next_scheduler_wait =
                                Some(next_scheduler_wait.map_or(wait, |current| current.min(wait)));
                            break;
                        }
                        // Frontier currently empty â€” tasks may still add URLs.
                        SchedulerPoll::Empty => break,
                    }
                }
            }

            let next_wake = match (next_scheduler_wait, budgets.remaining_duration(start)) {
                (Some(scheduler_wait), Some(budget_wait)) => Some(scheduler_wait.min(budget_wait)),
                (Some(scheduler_wait), None) => Some(scheduler_wait),
                (None, Some(budget_wait)) => Some(budget_wait),
                (None, None) => None,
            };

            if join_set.is_empty() {
                if shutting_down {
                    break;
                }
                if scheduler.is_empty().await {
                    break;
                }
                tokio::time::sleep(next_wake.unwrap_or(std::time::Duration::from_millis(10))).await;
                continue;
            }

            let scheduler_sleep = tokio::time::sleep(
                next_wake.unwrap_or(std::time::Duration::from_secs(24 * 60 * 60)),
            );
            tokio::pin!(scheduler_sleep);

            tokio::select! {
                _ = &mut scheduler_sleep, if next_wake.is_some() => {
                    continue;
                }
                _ = &mut shutdown, if !shutting_down => {
                    shutting_down = true;
                    stats.interrupted = true;
                    stats.stop_reason = Some(crate::stats::StopReason::Interrupted);
                }
                result = join_set.join_next_with_id() => {
                    match result {
                        Some(Ok((task_id, (queued, Ok(output))))) => {
                            task_context.remove(&task_id);
                            scheduler.finish(&queued).await;
                            stats.record_completed(&domain_key(queued.request.url()));
                            stats.pages_crawled += 1;
                            stats.items_scraped += output.item_count;
                            stats.bytes_downloaded += output.bytes_downloaded;
                            if is_cancelled(&stream_cancelled) {
                                shutting_down = true;
                                stats.interrupted = true;
                                stats.stop_reason = Some(crate::stats::StopReason::Interrupted);
                            }
                            update_live_stats(metrics_interval, &live_stats, &stats, start).await;

                            if !shutting_down && budgets.mark_if_reached(&mut stats, start) {
                                shutting_down = true;
                            }

                            if !shutting_down {
                                for (follow_request, follow_depth) in output.follows {
                                    if should_enqueue(&follow_request, follow_depth, spider.as_ref()) {
                                        let domain = domain_key(follow_request.url());
                                        let url = follow_request.url().to_string();
                                        if scheduler.push_request(follow_request, follow_depth).await {
                                            stats.record_scheduled(&domain);
                                            if let Some(events) = &events {
                                                events.emit(crate::events::CrawlEvent::RequestScheduled {
                                                    spider: spider.name().to_string(),
                                                    spider_index: None,
                                                    url,
                                                    domain,
                                                    depth: follow_depth,
                                                });
                                            }
                                        } else {
                                            stats.record_deduped(&domain);
                                            if let Some(events) = &events {
                                                events.emit(crate::events::CrawlEvent::RequestSkipped {
                                                    spider: spider.name().to_string(),
                                                    spider_index: None,
                                                    url,
                                                    domain,
                                                    depth: follow_depth,
                                                    reason: crate::events::RequestSkipReason::Duplicate,
                                                });
                                            }
                                        }
                                        update_live_stats(metrics_interval, &live_stats, &stats, start).await;
                                    } else if let Some(reason) =
                                        skip_reason(&follow_request, follow_depth, spider.as_ref())
                                        && let Some(events) = &events
                                    {
                                        let url = follow_request.url().to_string();
                                        events.emit(crate::events::CrawlEvent::RequestSkipped {
                                            spider: spider.name().to_string(),
                                            spider_index: None,
                                            domain: domain_key(&url),
                                            url,
                                            depth: follow_depth,
                                            reason,
                                        });
                                    }
                                }
                            }
                        }
                        Some(Ok((task_id, (queued, Err(e))))) => {
                            task_context.remove(&task_id);
                            scheduler.finish(&queued).await;
                            let url = queued.request.url().to_string();
                            // Notify all middleware of the permanent failure.
                            for mw in middleware.iter() {
                                mw.on_error(&url, &e).await;
                            }
                            let domain = domain_key(&url);
                            let retry_policy_exhausted = retry_policy.max_attempts > 0
                                && retry_policy.is_retriable(&e)
                                && queued.retry_count >= retry_policy.max_attempts;
                            if !shutting_down
                                && queued.retry_count < retry_policy.max_attempts
                                && retry_policy.is_retriable(&e)
                            {
                                let retry_delay_hint =
                                    middleware.iter().find_map(|mw| mw.retry_delay(&url, &e));
                                let delay = retry_policy
                                    .delay_for_with_hint(queued.retry_count, retry_delay_hint);
                                stats.record_retry(&domain);
                                update_live_stats(metrics_interval, &live_stats, &stats, start).await;
                                if let Some(events) = &events {
                                    events.emit(crate::events::CrawlEvent::RequestRetried {
                                        spider: spider.name().to_string(),
                                        spider_index: None,
                                        url: url.clone(),
                                        domain: domain.clone(),
                                        depth: queued.depth,
                                        attempt: queued.retry_count + 1,
                                        max_attempts: retry_policy.max_attempts,
                                        delay,
                                        error_kind: e.kind(),
                                    });
                                }
                                tracing::info!(
                                    target: target::REQUEST,
                                    event = event::REQUEST_RETRY,
                                    spider = spider.name(),
                                    url = %url,
                                    domain = %domain,
                                    depth = queued.depth,
                                    attempt = queued.retry_count + 1,
                                    max_attempts = retry_policy.max_attempts,
                                    retry_in_ms = delay.as_millis(),
                                    error = %e,
                                    error_kind = e.kind().as_str(),
                                    "request.retry"
                                );
                                scheduler
                                    .push_request_force(
                                        FrontierRequest::new(
                                            queued.request,
                                            queued.depth,
                                            queued.retry_count + 1,
                                        )
                                        .scheduled_after(delay),
                                    )
                                    .await;
                                continue;
                            }

                            let mut retry_exhausted_recorded = false;
                            if retry_policy_exhausted {
                                stats.record_retry_exhausted(&domain);
                                retry_exhausted_recorded = true;
                            }
                            stats.record_error_kind(&domain, e.kind());
                            if let Some(events) = &events {
                                events.emit(crate::events::CrawlEvent::RequestFailed {
                                    spider: spider.name().to_string(),
                                    spider_index: None,
                                    url: url.clone(),
                                    domain: domain.clone(),
                                    depth: queued.depth,
                                    attempt: queued.retry_count,
                                    error_kind: e.kind(),
                                    retry_exhausted: retry_policy_exhausted,
                                });
                            }
                            update_live_stats(metrics_interval, &live_stats, &stats, start).await;
                            if !shutting_down && budgets.mark_if_reached(&mut stats, start) {
                                shutting_down = true;
                            }
                            match spider.on_error(&url, &e) {
                                ErrorPolicy::Abort => {
                                    error!(
                                        target: target::CRAWL,
                                        event = event::CRAWL_ABORT,
                                        spider = spider.name(),
                                        url = %url,
                                        domain = %domain,
                                        depth = queued.depth,
                                        attempt = queued.retry_count,
                                        error = %e,
                                        error_kind = e.kind().as_str(),
                                        "crawl.abort"
                                    );
                                    return Err(e);
                                }
                                ErrorPolicy::Retry(max) if queued.retry_count < max => {
                                    if let Some(events) = &events {
                                        events.emit(crate::events::CrawlEvent::RequestRetried {
                                            spider: spider.name().to_string(),
                                            spider_index: None,
                                            url: url.clone(),
                                            domain: domain.clone(),
                                            depth: queued.depth,
                                            attempt: queued.retry_count + 1,
                                            max_attempts: max,
                                            delay: std::time::Duration::ZERO,
                                            error_kind: e.kind(),
                                        });
                                    }
                                    tracing::info!(
                                        target: target::REQUEST,
                                        event = event::REQUEST_RETRY,
                                        spider = spider.name(),
                                        url = %url,
                                        domain = %domain,
                                        depth = queued.depth,
                                        attempt = queued.retry_count + 1,
                                        max_attempts = max,
                                        retry_in_ms = 0u128,
                                        error = %e,
                                        error_kind = e.kind().as_str(),
                                        "request.retry"
                                    );
                                    if !shutting_down {
                                        stats.record_retry(&domain);
                                        update_live_stats(metrics_interval, &live_stats, &stats, start).await;
                                        scheduler.push_request_force(FrontierRequest::new(
                                            queued.request,
                                            queued.depth,
                                            queued.retry_count + 1,
                                        )).await;
                                    }
                                }
                                ErrorPolicy::Retry(_) => {
                                    if !retry_exhausted_recorded {
                                        stats.record_retry_exhausted(&domain);
                                        update_live_stats(metrics_interval, &live_stats, &stats, start).await;
                                    }
                                    tracing::warn!(
                                        target: target::REQUEST,
                                        event = event::REQUEST_RETRY_EXHAUSTED,
                                        spider = spider.name(),
                                        url = %url,
                                        domain = %domain,
                                        depth = queued.depth,
                                        attempt = queued.retry_count,
                                        max_attempts = retry_policy.max_attempts,
                                        error = %e,
                                        error_kind = e.kind().as_str(),
                                        "request.retry_exhausted"
                                    );
                                }
                                ErrorPolicy::Skip => {
                                    tracing::warn!(
                                        target: target::REQUEST,
                                        event = event::REQUEST_SKIP,
                                        spider = spider.name(),
                                        url = %url,
                                        domain = %domain,
                                        depth = queued.depth,
                                        attempt = queued.retry_count,
                                        error = %e,
                                        error_kind = e.kind().as_str(),
                                        "request.skip"
                                    );
                                }
                            }
                        }
                        Some(Err(join_err)) => {
                            if let Some(queued) = task_context.remove(&join_err.id()) {
                                scheduler.finish(&queued).await;
                                stats.record_error(&domain_key(queued.request.url()));
                                if let Some(events) = &events {
                                    let url = queued.request.url().to_string();
                                    events.emit(crate::events::CrawlEvent::TaskPanicked {
                                        spider: spider.name().to_string(),
                                        spider_index: None,
                                        domain: Some(domain_key(&url)),
                                        url: Some(url),
                                        depth: Some(queued.depth),
                                    });
                                }
                                update_live_stats(metrics_interval, &live_stats, &stats, start).await;
                                if !shutting_down && budgets.mark_if_reached(&mut stats, start) {
                                    shutting_down = true;
                                }
                            } else {
                                stats.errors += 1;
                                if let Some(events) = &events {
                                    events.emit(crate::events::CrawlEvent::TaskPanicked {
                                        spider: spider.name().to_string(),
                                        spider_index: None,
                                        domain: None,
                                        url: None,
                                        depth: None,
                                    });
                                }
                                update_live_stats(metrics_interval, &live_stats, &stats, start).await;
                                if !shutting_down && budgets.mark_if_reached(&mut stats, start) {
                                    shutting_down = true;
                                }
                            }
                            error!(
                                target: target::CRAWL,
                                event = event::CRAWL_TASK_PANIC,
                                spider = spider.name(),
                                error = %join_err,
                                "crawl.task_panic"
                            );
                        }
                        None => break,
                    }

                    if shutting_down && join_set.is_empty() {
                        break;
                    }
                }
            }
        }

        scheduler.flush().await?;
        store.flush().await?;
        stats.duration = start.elapsed();
        if stats.stop_reason.is_none() {
            stats.stop_reason = if stats.interrupted {
                Some(crate::stats::StopReason::Interrupted)
            } else {
                Some(crate::stats::StopReason::FrontierExhausted)
            };
        }

        // close() errors are intentionally not propagated â€” the crawl and store
        // flush completed successfully. Cleanup failures are logged only.
        if let Err(e) = spider.close(&stats).await {
            tracing::error!(
                target: target::CRAWL,
                event = event::SPIDER_CLOSE_FAILED,
                spider = spider.name(),
                error = %e,
                "spider.close_failed"
            );
        }

        let rps = if stats.duration.as_secs_f64() > 0.0 {
            stats.pages_crawled as f64 / stats.duration.as_secs_f64()
        } else {
            0.0
        };
        info!(
            target: target::CRAWL,
            event = event::CRAWL_COMPLETE,
            spider = spider.name(),
            pages = stats.pages_crawled,
            items = stats.items_scraped,
            errors = stats.errors,
            scheduled = stats.scheduled,
            deduped = stats.deduped,
            retries = stats.retries,
            retry_exhausted = stats.retry_exhausted,
            robots_blocked = stats.robots_blocked,
            bytes = stats.bytes_downloaded,
            duration_secs = stats.duration.as_secs_f64(),
            pages_per_sec = format!("{rps:.1}"),
            interrupted = stats.interrupted,
            stop_reason = stats.stop_reason.map(crate::stats::StopReason::as_str),
            error_kinds = ?stats.error_kinds,
            "crawl.complete"
        );

        if let Some(events) = &events {
            events.emit(crate::events::CrawlEvent::CrawlFinished {
                spider: spider.name().to_string(),
                spider_index: None,
                stop_reason: stats.stop_reason,
                report: crate::stats::CrawlReport::from(stats.clone()),
            });
        }

        Ok(stats)
    }
}
