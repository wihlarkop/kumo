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
    scheduler::{CrawlScheduler, ScheduledRequest, SchedulerPoll},
    stats::CrawlStats,
    store::ItemStore,
};

use super::{
    USER_AGENT,
    budget::CrawlBudgets,
    builder::CrawlEngine,
    erased::ErasedSpider,
    setup::{
        FetcherArgs, build_http_client, build_raw_fetcher, build_robots_cache, wrap_with_cache,
    },
    task::{RequestTaskOutput, TaskContext, process_request_once, should_enqueue, skip_reason},
};

impl CrawlEngine {
    /// Run all spiders registered via [`add_spider`](Self::add_spider) concurrently
    /// within the same engine (shared middleware, store, and concurrency limit).
    ///
    /// Returns one [`CrawlStats`] per spider, in registration order.
    pub async fn run_all(self) -> Result<Vec<CrawlStats>, KumoError> {
        if self.spiders.is_empty() {
            return Ok(Vec::new());
        }

        let start = std::time::Instant::now();
        let observer = crate::hooks::CrawlObserver::new(
            self.events.clone(),
            crate::hooks::HookDispatcher::new(self.hooks, self.hook_error_policy),
        );
        let budgets = CrawlBudgets {
            max_pages: self.max_pages,
            max_items: self.max_items,
            max_duration: self.max_duration,
            max_errors: self.max_errors,
        };
        let n = self.spiders.len();
        let politeness_policy = self.politeness_policy;
        let fingerprint_policy = self.fingerprint_policy;
        let stats_checkpoint = self.stats_checkpoint;

        let spider_entries: Vec<(Arc<dyn ErasedSpider>, Arc<CrawlScheduler>)> = self
            .spiders
            .into_iter()
            .map(|sp| {
                let frontier: Arc<dyn Frontier> = Arc::new(MemoryFrontier::new(self.max_urls));
                let scheduler = Arc::new(
                    CrawlScheduler::from_arc(frontier, politeness_policy.clone())
                        .with_fingerprint_policy(fingerprint_policy.clone()),
                );
                (sp, scheduler)
            })
            .collect();

        let store = self
            .store
            .unwrap_or_else(|| Arc::new(crate::store::stdout::StdoutStore));
        let buffered_store = self
            .store_buffer
            .map(|config| Arc::new(crate::store::BufferedStore::new(store.clone(), config)));
        let store: Arc<dyn ItemStore> = match buffered_store.clone() {
            Some(buffered) => buffered,
            None => store,
        };
        let middleware: Arc<Vec<Arc<dyn Middleware>>> = Arc::new(self.middleware);
        let pipelines: Arc<Vec<Arc<dyn Pipeline>>> = Arc::new(self.pipelines);
        let concurrency = self.concurrency;
        let retry_policy = self.retry_policy;

        let client_policy = crate::fetch::client_policy::HttpClientPolicy::new(
            concurrency,
            self.request_timeout,
            USER_AGENT,
        );
        let client = build_http_client(&client_policy, self.http_client_builder)?;
        let fetcher = build_raw_fetcher(FetcherArgs {
            fetcher_override: self.fetcher_override,
            client: client.clone(),
            client_policy,
            concurrency,
            #[cfg(feature = "stealth")]
            stealth_profile: self.stealth_profile,
            #[cfg(feature = "browser")]
            browser: self.browser,
            #[cfg(feature = "browser")]
            browser_fallback: self.browser_fallback,
        })
        .await?;
        let fetcher = wrap_with_cache(fetcher, self.cache_dir, self.cache_ttl)?;
        let robots_cache = build_robots_cache(self.respect_robots, self.robots_ttl);

        for (spider, _) in &spider_entries {
            spider.open().await?;
        }

        let mut stats_vec: Vec<CrawlStats> = (0..n).map(|_| CrawlStats::default()).collect();

        for (idx, (spider, scheduler)) in spider_entries.iter().enumerate() {
            info!(
                target: target::CRAWL,
                event = event::CRAWL_START,
                spider = spider.name(),
                spider_index = idx,
                start_urls = spider.start_urls().len(),
                "crawl.start"
            );
            observer
                .notify_with(|| crate::events::CrawlEvent::CrawlStarted {
                    spider: spider.name().to_string(),
                    spider_index: Some(idx),
                    start_urls: spider.start_urls().len(),
                })
                .await?;
            for url in spider.start_urls() {
                let request = CrawlRequest::get(url.clone());
                let domain = request.stats_domain().to_string();
                let stats = &mut stats_vec[idx];
                if scheduler.push_request(request, 0).await {
                    stats.record_scheduled(&domain);
                    #[cfg(feature = "otel")]
                    crate::otel::record_request_scheduled(spider.name(), Some(idx), &domain);
                    observer
                        .notify_with(|| crate::events::CrawlEvent::RequestScheduled {
                            spider: spider.name().to_string(),
                            spider_index: Some(idx),
                            url,
                            domain,
                            depth: 0,
                        })
                        .await?;
                } else {
                    stats.record_deduped(&domain);
                    #[cfg(feature = "otel")]
                    {
                        crate::otel::record_request_deduped(spider.name(), Some(idx), &domain);
                        crate::otel::record_request_skipped(
                            spider.name(),
                            Some(idx),
                            &domain,
                            crate::events::RequestSkipReason::Duplicate,
                        );
                    }
                    observer
                        .notify_with(|| crate::events::CrawlEvent::RequestSkipped {
                            spider: spider.name().to_string(),
                            spider_index: Some(idx),
                            url,
                            domain,
                            depth: 0,
                            reason: crate::events::RequestSkipReason::Duplicate,
                        })
                        .await?;
                }
            }
        }

        type MultiTaskResult = (
            usize,
            ScheduledRequest,
            Result<RequestTaskOutput, KumoError>,
        );
        let mut join_set: JoinSet<MultiTaskResult> = JoinSet::new();
        let mut task_context = HashMap::new();

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
        let mut fill_cursor = 0usize;

        loop {
            for stats in &mut stats_vec {
                if stats.stop_reason.is_none() && budgets.mark_if_reached(stats, start) {
                    // This spider is done, but other spiders may still have work.
                }
            }

            let mut next_scheduler_wait: Option<std::time::Duration> = None;

            if !shutting_down {
                'fill: while join_set.len() < concurrency {
                    let mut any_popped = false;
                    for attempt in 0..n {
                        let idx = (fill_cursor + attempt) % n;
                        if stats_vec[idx].stop_reason.is_some() {
                            continue;
                        }
                        let (spider, scheduler) = &spider_entries[idx];
                        match scheduler.poll_ready().await {
                            SchedulerPoll::Ready(queued) => {
                                let scheduled = *queued;
                                let queued = scheduled.queued.clone();
                                if let Some(ref cache) = robots_cache
                                    && !cache.is_request_allowed(&client, queued.request()).await
                                {
                                    observer
                                        .notify_with(|| crate::events::CrawlEvent::RequestSkipped {
                                            spider: spider.name().to_string(),
                                            spider_index: Some(idx),
                                            domain: queued.request.stats_domain().to_string(),
                                            url: queued.request.url().to_string(),
                                            depth: queued.depth,
                                            reason: crate::events::RequestSkipReason::RobotsTxt,
                                        })
                                        .await?;
                                    tracing::debug!(
                                        target: target::REQUEST,
                                        event = event::REQUEST_ROBOTS_BLOCKED,
                                        spider = spider.name(),
                                        spider_index = idx,
                                        url = %queued.request.url(),
                                        domain = %queued.request.stats_domain(),
                                        depth = queued.depth,
                                        attempt = queued.retry_count,
                                        "request.robots_blocked"
                                    );
                                    stats_vec[idx]
                                        .record_robots_blocked(queued.request.stats_domain());
                                    #[cfg(feature = "otel")]
                                    crate::otel::record_robots_blocked(
                                        spider.name(),
                                        Some(idx),
                                        queued.request.stats_domain(),
                                    );
                                    scheduler.finish(&queued).await;
                                    scheduler.ack(&scheduled).await?;
                                    continue;
                                }
                                if let Some(ref cache) = robots_cache
                                    && let Some(delay) =
                                        cache.request_crawl_delay(&client, queued.request()).await
                                {
                                    scheduler
                                        .observe_request_robots_crawl_delay(queued.request(), delay)
                                        .await;
                                }
                                let ctx = TaskContext {
                                    spider: spider.clone(),
                                    spider_index: Some(idx),
                                    store: store.clone(),
                                    middleware: middleware.clone(),
                                    pipelines: pipelines.clone(),
                                    fetcher: fetcher.clone(),
                                    observer: observer.clone(),
                                    stream_cancelled: None,
                                };
                                let task_scheduled = scheduled.clone();
                                let task_id = join_set
                                    .spawn(async move {
                                        let result =
                                            process_request_once(&task_scheduled.queued, &ctx)
                                                .await;
                                        (idx, task_scheduled, result)
                                    })
                                    .id();
                                task_context.insert(task_id, (idx, scheduled));
                                fill_cursor = idx + 1;
                                any_popped = true;
                                break;
                            }
                            SchedulerPoll::Pending(wait) => {
                                next_scheduler_wait = Some(
                                    next_scheduler_wait.map_or(wait, |current| current.min(wait)),
                                );
                            }
                            SchedulerPoll::Empty => {}
                        }
                    }
                    if !any_popped {
                        break 'fill;
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
                let mut all_empty = true;
                for (idx, (_, scheduler)) in spider_entries.iter().enumerate() {
                    if stats_vec[idx].stop_reason.is_none() && !scheduler.is_empty().await {
                        all_empty = false;
                        break;
                    }
                }
                if all_empty {
                    for stats in &mut stats_vec {
                        if stats.stop_reason.is_none() {
                            stats.stop_reason = if stats.interrupted {
                                Some(crate::stats::StopReason::Interrupted)
                            } else {
                                Some(crate::stats::StopReason::FrontierExhausted)
                            };
                        }
                    }
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
                    for s in &mut stats_vec {
                        s.interrupted = true;
                        if s.stop_reason.is_none() {
                            s.stop_reason = Some(crate::stats::StopReason::Interrupted);
                        }
                    }
                }
                result = join_set.join_next_with_id() => {
                    match result {
                        Some(Ok((task_id, (spider_idx, scheduled, Ok(output))))) => {
                            task_context.remove(&task_id);
                            let queued = scheduled.queued.clone();
                            let (_, scheduler) = &spider_entries[spider_idx];
                            scheduler.finish(&queued).await;
                            let stats = &mut stats_vec[spider_idx];
                            stats.record_completed(queued.request.stats_domain());
                            stats.pages_crawled += 1;
                            stats.items_scraped += output.item_count;
                            stats.bytes_downloaded += output.bytes_downloaded;
                            stats.timings += output.timings;
                            stats.record_fetch_stats(output.fetch_stats);
                            #[cfg(feature = "otel")]
                            crate::otel::record_fetch_latency(
                                spider_entries[spider_idx].0.name(),
                                Some(spider_idx),
                                output.timings.fetch,
                            );
                            let budget_reached = budgets.mark_if_reached(stats, start);
                            if !shutting_down {
                                let (spider, scheduler) = &spider_entries[spider_idx];
                                if !budget_reached {
                                    for (follow_request, follow_depth) in output.follows {
                                        if should_enqueue(&follow_request, follow_depth, spider.as_ref()) {
                                            let domain = follow_request.stats_domain().to_string();
                                            let event_url = observer
                                                .is_enabled()
                                                .then(|| follow_request.url().to_string());
                                            if scheduler.push_request(follow_request, follow_depth).await {
                                                stats.record_scheduled(&domain);
                                                #[cfg(feature = "otel")]
                                                crate::otel::record_request_scheduled(
                                                    spider.name(),
                                                    Some(spider_idx),
                                                    &domain,
                                                );
                                                observer
                                                    .notify_with(|| crate::events::CrawlEvent::RequestScheduled {
                                                        spider: spider.name().to_string(),
                                                        spider_index: Some(spider_idx),
                                                        url: event_url.expect(
                                                            "enabled observer has a request URL",
                                                        ),
                                                        domain,
                                                        depth: follow_depth,
                                                    })
                                                    .await?;
                                            } else {
                                                stats.record_deduped(&domain);
                                                #[cfg(feature = "otel")]
                                                {
                                                    crate::otel::record_request_deduped(
                                                        spider.name(),
                                                        Some(spider_idx),
                                                        &domain,
                                                    );
                                                    crate::otel::record_request_skipped(
                                                        spider.name(),
                                                        Some(spider_idx),
                                                        &domain,
                                                        crate::events::RequestSkipReason::Duplicate,
                                                    );
                                                }
                                                observer
                                                    .notify_with(|| crate::events::CrawlEvent::RequestSkipped {
                                                        spider: spider.name().to_string(),
                                                        spider_index: Some(spider_idx),
                                                        url: event_url.expect(
                                                            "enabled observer has a request URL",
                                                        ),
                                                        domain,
                                                        depth: follow_depth,
                                                        reason: crate::events::RequestSkipReason::Duplicate,
                                                    })
                                                    .await?;
                                            }
                                        } else if let Some(reason) =
                                            skip_reason(&follow_request, follow_depth, spider.as_ref())
                                        {
                                            observer
                                                .notify_with(|| crate::events::CrawlEvent::RequestSkipped {
                                                spider: spider.name().to_string(),
                                                spider_index: Some(spider_idx),
                                                domain: follow_request.stats_domain().to_string(),
                                                url: follow_request.url().to_string(),
                                                depth: follow_depth,
                                                reason,
                                            })
                                                .await?;
                                            #[cfg(feature = "otel")]
                                            crate::otel::record_request_skipped(
                                                spider.name(),
                                                Some(spider_idx),
                                                follow_request.stats_domain(),
                                                reason,
                                            );
                                        }
                                    }
                                }
                            }
                            scheduler.ack(&scheduled).await?;
                        }
                        Some(Ok((task_id, (spider_idx, scheduled, Err(e))))) => {
                            task_context.remove(&task_id);
                            let queued = scheduled.queued.clone();
                            let (_, scheduler) = &spider_entries[spider_idx];
                            scheduler.finish(&queued).await;
                            let url = queued.request.url();
                            if e.kind() == crate::error::KumoErrorKind::Hook {
                                scheduler.release(&scheduled).await?;
                                return Err(e);
                            }
                            for mw in middleware.iter() {
                                mw.on_error(url, &e).await;
                            }
                            let domain = queued.request.stats_domain();
                            let retry_policy_exhausted = retry_policy.max_attempts > 0
                                && retry_policy.is_retriable(&e)
                                && queued.retry_count >= retry_policy.max_attempts;
                            let (spider, scheduler) = &spider_entries[spider_idx];
                            if !shutting_down
                                && queued.retry_count < retry_policy.max_attempts
                                && retry_policy.is_retriable(&e)
                            {
                                let retry_delay_hint =
                                    middleware.iter().find_map(|mw| mw.retry_delay(url, &e));
                                let delay = retry_policy
                                    .delay_for_with_hint(queued.retry_count, retry_delay_hint);
                                stats_vec[spider_idx].record_retry(domain);
                                #[cfg(feature = "otel")]
                                crate::otel::record_retry_scheduled(
                                    spider.name(),
                                    Some(spider_idx),
                                    domain,
                                );
                                observer
                                    .notify_with(|| crate::events::CrawlEvent::RequestRetried {
                                        spider: spider.name().to_string(),
                                        spider_index: Some(spider_idx),
                                        url: url.to_string(),
                                        domain: domain.to_string(),
                                        depth: queued.depth,
                                        attempt: queued.retry_count + 1,
                                        max_attempts: retry_policy.max_attempts,
                                        delay,
                                        error_kind: e.kind(),
                                    })
                                    .await?;
                                tracing::info!(
                                    target: target::REQUEST,
                                    event = event::REQUEST_RETRY,
                                    spider = spider.name(),
                                    spider_index = spider_idx,
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
                                            queued.request.clone(),
                                            queued.depth,
                                            queued.retry_count + 1,
                                        )
                                        .scheduled_after(delay),
                                    )
                                    .await;
                                scheduler.ack(&scheduled).await?;
                                continue;
                            }

                            let mut retry_exhausted_recorded = false;
                            if retry_policy_exhausted {
                                stats_vec[spider_idx].record_retry_exhausted(domain);
                                #[cfg(feature = "otel")]
                                crate::otel::record_retry_exhausted(
                                    spider.name(),
                                    Some(spider_idx),
                                    domain,
                                );
                                retry_exhausted_recorded = true;
                            }
                            stats_vec[spider_idx].record_error_kind(domain, e.kind());
                            #[cfg(feature = "otel")]
                            crate::otel::record_request_failed(
                                spider.name(),
                                Some(spider_idx),
                                domain,
                                e.kind().as_str(),
                            );
                            observer
                                .notify_with(|| crate::events::CrawlEvent::RequestFailed {
                                    spider: spider.name().to_string(),
                                    spider_index: Some(spider_idx),
                                    url: url.to_string(),
                                    domain: domain.to_string(),
                                    depth: queued.depth,
                                    attempt: queued.retry_count,
                                    error_kind: e.kind(),
                                    retry_exhausted: retry_policy_exhausted,
                                })
                                .await?;
                            budgets.mark_if_reached(&mut stats_vec[spider_idx], start);
                            match spider.on_error(url, &e) {
                                ErrorPolicy::Abort => {
                                    error!(
                                        target: target::CRAWL,
                                        event = event::CRAWL_ABORT,
                                        spider = spider.name(),
                                        spider_index = spider_idx,
                                        url = %url,
                                        domain = %domain,
                                        depth = queued.depth,
                                        attempt = queued.retry_count,
                                        error = %e,
                                        error_kind = e.kind().as_str(),
                                        "crawl.abort"
                                    );
                                    scheduler.release(&scheduled).await?;
                                    return Err(e);
                                }
                                ErrorPolicy::Retry(max) if queued.retry_count < max => {
                                    observer
                                        .notify_with(|| crate::events::CrawlEvent::RequestRetried {
                                            spider: spider.name().to_string(),
                                            spider_index: Some(spider_idx),
                                            url: url.to_string(),
                                            domain: domain.to_string(),
                                            depth: queued.depth,
                                            attempt: queued.retry_count + 1,
                                            max_attempts: max,
                                            delay: std::time::Duration::ZERO,
                                            error_kind: e.kind(),
                                        })
                                        .await?;
                                    tracing::info!(
                                        target: target::REQUEST,
                                        event = event::REQUEST_RETRY,
                                        spider = spider.name(),
                                        spider_index = spider_idx,
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
                                    if !shutting_down
                                        && stats_vec[spider_idx].stop_reason.is_none()
                                    {
                                        stats_vec[spider_idx].record_retry(domain);
                                        #[cfg(feature = "otel")]
                                        crate::otel::record_retry_scheduled(
                                            spider.name(),
                                            Some(spider_idx),
                                            domain,
                                        );
                                        scheduler.push_request_force(FrontierRequest::new(
                                            queued.request.clone(),
                                            queued.depth,
                                            queued.retry_count + 1,
                                        )).await;
                                        scheduler.ack(&scheduled).await?;
                                        continue;
                                    }
                                }
                                ErrorPolicy::Retry(_) => {
                                    if !retry_exhausted_recorded {
                                        stats_vec[spider_idx].record_retry_exhausted(domain);
                                        #[cfg(feature = "otel")]
                                        crate::otel::record_retry_exhausted(
                                            spider.name(),
                                            Some(spider_idx),
                                            domain,
                                        );
                                        retry_exhausted_recorded = true;
                                    }
                                    tracing::warn!(
                                        target: target::REQUEST,
                                        event = event::REQUEST_RETRY_EXHAUSTED,
                                        spider = spider.name(),
                                        spider_index = spider_idx,
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
                                        spider_index = spider_idx,
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
                            if retry_exhausted_recorded {
                                scheduler
                                    .dead_letter(
                                        &scheduled,
                                        crate::frontier::DeadLetterReason::RetryExhausted,
                                    )
                                    .await?;
                            } else {
                                scheduler.ack(&scheduled).await?;
                            }
                        }
                        Some(Err(join_err)) => {
                            if let Some((spider_idx, queued)) = task_context.remove(&join_err.id()) {
                                let (_, scheduler) = &spider_entries[spider_idx];
                                scheduler.finish(&queued.queued).await;
                                scheduler
                                    .dead_letter(&queued, crate::frontier::DeadLetterReason::Failed)
                                    .await?;
                                let (spider, _) = &spider_entries[spider_idx];
                                stats_vec[spider_idx]
                                    .record_error(queued.queued.request.stats_domain());
                                #[cfg(feature = "otel")]
                                crate::otel::record_request_failed(
                                    spider.name(),
                                    Some(spider_idx),
                                    queued.queued.request.stats_domain(),
                                    "task_panic",
                                );
                                observer
                                    .notify_with(|| crate::events::CrawlEvent::TaskPanicked {
                                        spider: spider.name().to_string(),
                                        spider_index: Some(spider_idx),
                                        domain: Some(queued.queued.request.stats_domain().to_string()),
                                        url: Some(queued.queued.request.url().to_string()),
                                        depth: Some(queued.queued.depth),
                                    })
                                    .await?;
                                budgets.mark_if_reached(&mut stats_vec[spider_idx], start);
                            }
                            error!(
                                target: target::CRAWL,
                                event = event::CRAWL_TASK_PANIC,
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

        for (_, scheduler) in &spider_entries {
            scheduler.flush().await?;
        }
        store.flush().await?;
        if let Some(buffered) = buffered_store {
            let store_stats = buffered.stats();
            for stats in &mut stats_vec {
                stats.store = store_stats;
            }
        }
        let elapsed = start.elapsed();

        for (i, (spider, _)) in spider_entries.iter().enumerate() {
            stats_vec[i].duration = elapsed;
            if stats_vec[i].stop_reason.is_none() {
                stats_vec[i].stop_reason = if stats_vec[i].interrupted {
                    Some(crate::stats::StopReason::Interrupted)
                } else {
                    Some(crate::stats::StopReason::FrontierExhausted)
                };
            }
            if let Err(e) = spider.close(&stats_vec[i]).await {
                tracing::error!(
                    target: target::CRAWL,
                    event = event::SPIDER_CLOSE_FAILED,
                    spider = spider.name(),
                    spider_index = i,
                    error = %e,
                    "spider.close_failed"
                );
            }
            let rps = if elapsed.as_secs_f64() > 0.0 {
                stats_vec[i].pages_crawled as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };
            info!(
                target: target::CRAWL,
                event = event::CRAWL_COMPLETE,
                spider = spider.name(),
                spider_index = i,
                pages = stats_vec[i].pages_crawled,
                items = stats_vec[i].items_scraped,
                errors = stats_vec[i].errors,
                scheduled = stats_vec[i].scheduled,
                deduped = stats_vec[i].deduped,
                retries = stats_vec[i].retries,
                retry_exhausted = stats_vec[i].retry_exhausted,
                robots_blocked = stats_vec[i].robots_blocked,
                store_buffered = stats_vec[i].store.buffered,
                store_queued = stats_vec[i].store.queued,
                store_written = stats_vec[i].store.written,
                store_batches = stats_vec[i].store.batches,
                store_failed_writes = stats_vec[i].store.failed_writes,
                store_failed_batches = stats_vec[i].store.failed_batches,
                store_queue_full_waits = stats_vec[i].store.queue_full_waits,
                store_queue_wait_avg_secs =
                    stats_vec[i].store.average_queue_wait_per_item().as_secs_f64(),
                store_write_avg_batch_secs =
                    stats_vec[i].store.average_write_per_batch().as_secs_f64(),
                bytes = stats_vec[i].bytes_downloaded,
                pages_per_sec = format!("{rps:.1}"),
                stop_reason = stats_vec[i].stop_reason.map(crate::stats::StopReason::as_str),
                error_kinds = ?stats_vec[i].error_kinds,
                "crawl.complete"
            );
            let report = crate::stats::CrawlReport::from(stats_vec[i].clone());
            #[cfg(feature = "otel")]
            crate::otel::record_crawl_report(spider.name(), Some(i), &report);

            observer
                .notify_with(|| crate::events::CrawlEvent::CrawlFinished {
                    spider: spider.name().to_string(),
                    spider_index: Some(i),
                    stop_reason: stats_vec[i].stop_reason,
                    report,
                })
                .await?;
        }

        if let Some(config) = &stats_checkpoint {
            crate::stats_checkpoint::write_stats_checkpoints(config, stats_vec.clone()).await?;
        }

        Ok(stats_vec)
    }
}
