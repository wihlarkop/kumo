use std::sync::Arc;

use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    error::{ErrorPolicy, KumoError},
    frontier::{Frontier, memory::MemoryFrontier},
    middleware::Middleware,
    pipeline::Pipeline,
    request::{CrawlRequest, FrontierRequest},
    scheduler::{CrawlScheduler, SchedulerPoll},
    stats::{CrawlStats, domain_key},
};

use super::{
    builder::CrawlEngine,
    erased::ErasedSpider,
    setup::{
        FetcherArgs, build_http_client, build_raw_fetcher, build_robots_cache, wrap_with_cache,
    },
    task::{TaskContext, process_request_once, should_enqueue},
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
        let n = self.spiders.len();
        let politeness_policy = self.politeness_policy;
        let fingerprint_policy = self.fingerprint_policy;

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
        let middleware: Arc<Vec<Arc<dyn Middleware>>> = Arc::new(self.middleware);
        let pipelines: Arc<Vec<Arc<dyn Pipeline>>> = Arc::new(self.pipelines);
        let concurrency = self.concurrency;
        let retry_policy = self.retry_policy;

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
        let robots_cache = build_robots_cache(self.respect_robots, self.robots_ttl);

        for (spider, _) in &spider_entries {
            spider.open().await?;
        }

        let mut stats_vec: Vec<CrawlStats> = (0..n).map(|_| CrawlStats::default()).collect();

        for (idx, (spider, scheduler)) in spider_entries.iter().enumerate() {
            info!(spider = spider.name(), "registering spider for multi-crawl");
            for url in spider.start_urls() {
                let domain = domain_key(&url);
                let stats = &mut stats_vec[idx];
                if scheduler.push_request(CrawlRequest::get(url), 0).await {
                    stats.record_scheduled(&domain);
                } else {
                    stats.record_deduped(&domain);
                }
            }
        }

        type MultiTaskResult = (
            usize,
            FrontierRequest,
            Result<(u64, u64, Vec<(CrawlRequest, usize)>), KumoError>,
        );
        let mut join_set: JoinSet<MultiTaskResult> = JoinSet::new();

        let shutdown = async {
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("ctrl-c received â€” finishing in-flight tasks then exiting");
            }
            #[cfg(target_arch = "wasm32")]
            std::future::pending::<()>().await
        };
        tokio::pin!(shutdown);
        let mut shutting_down = false;
        let mut fill_cursor = 0usize;

        loop {
            let mut next_scheduler_wait: Option<std::time::Duration> = None;

            if !shutting_down {
                'fill: while join_set.len() < concurrency {
                    let mut any_popped = false;
                    for attempt in 0..n {
                        let idx = (fill_cursor + attempt) % n;
                        let (spider, scheduler) = &spider_entries[idx];
                        match scheduler.poll_ready().await {
                            SchedulerPoll::Ready(queued) => {
                                let queued = *queued;
                                if let Some(ref cache) = robots_cache
                                    && !cache.is_allowed(&client, queued.request.url()).await
                                {
                                    tracing::debug!(url = %queued.request.url(), "blocked by robots.txt, skipping");
                                    stats_vec[idx]
                                        .record_robots_blocked(&domain_key(queued.request.url()));
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
                                    store: store.clone(),
                                    middleware: middleware.clone(),
                                    pipelines: pipelines.clone(),
                                    fetcher: fetcher.clone(),
                                    stream_cancelled: None,
                                };
                                join_set.spawn(async move {
                                    let result = process_request_once(queued.clone(), ctx).await;
                                    (idx, queued, result)
                                });
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

            if join_set.is_empty() {
                let mut all_empty = true;
                for (_, scheduler) in &spider_entries {
                    if !scheduler.is_empty().await {
                        all_empty = false;
                        break;
                    }
                }
                if all_empty {
                    break;
                }
                tokio::time::sleep(
                    next_scheduler_wait.unwrap_or(std::time::Duration::from_millis(10)),
                )
                .await;
                continue;
            }

            let scheduler_sleep = tokio::time::sleep(
                next_scheduler_wait.unwrap_or(std::time::Duration::from_secs(24 * 60 * 60)),
            );
            tokio::pin!(scheduler_sleep);

            tokio::select! {
                _ = &mut scheduler_sleep, if next_scheduler_wait.is_some() => {
                    continue;
                }
                _ = &mut shutdown, if !shutting_down => {
                    shutting_down = true;
                    for s in &mut stats_vec { s.interrupted = true; }
                }
                result = join_set.join_next() => {
                    match result {
                        Some(Ok((spider_idx, queued, Ok((item_count, bytes, follows))))) => {
                            let (_, scheduler) = &spider_entries[spider_idx];
                            scheduler.finish(&queued).await;
                            let stats = &mut stats_vec[spider_idx];
                            stats.record_completed(&domain_key(queued.request.url()));
                            stats.pages_crawled += 1;
                            stats.items_scraped += item_count;
                            stats.bytes_downloaded += bytes;
                            if !shutting_down {
                                let (spider, scheduler) = &spider_entries[spider_idx];
                                for (follow_request, follow_depth) in follows {
                                    if should_enqueue(&follow_request, follow_depth, spider.as_ref()) {
                                        let domain = domain_key(follow_request.url());
                                        if scheduler.push_request(follow_request, follow_depth).await {
                                            stats.record_scheduled(&domain);
                                        } else {
                                            stats.record_deduped(&domain);
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok((spider_idx, queued, Err(e)))) => {
                            let (_, scheduler) = &spider_entries[spider_idx];
                            scheduler.finish(&queued).await;
                            let url = queued.request.url().to_string();
                            for mw in middleware.iter() {
                                mw.on_error(&url, &e).await;
                            }
                            let (spider, scheduler) = &spider_entries[spider_idx];
                            if !shutting_down
                                && queued.retry_count < retry_policy.max_attempts
                                && retry_policy.is_retriable(&e)
                            {
                                let delay = retry_policy.delay_for(queued.retry_count);
                                stats_vec[spider_idx].record_retry(&domain_key(&url));
                                tracing::warn!(
                                    spider = spider.name(),
                                    url = %url,
                                    attempt = queued.retry_count + 1,
                                    max = retry_policy.max_attempts,
                                    retry_in_ms = delay.as_millis(),
                                    error = %e,
                                    "scheduling retry"
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

                            stats_vec[spider_idx].errors += 1;
                            stats_vec[spider_idx].record_failed(&domain_key(&url));
                            match spider.on_error(&url, &e) {
                                ErrorPolicy::Abort => {
                                    error!(url = %url, error = %e, "aborting crawl");
                                    return Err(e);
                                }
                                ErrorPolicy::Retry(max) if queued.retry_count < max => {
                                    tracing::warn!(
                                        spider = spider.name(),
                                        url = %url,
                                        attempt = queued.retry_count + 1,
                                        max,
                                        error = %e,
                                        "re-queuing failed URL"
                                    );
                                    if !shutting_down {
                                        stats_vec[spider_idx].record_retry(&domain_key(&url));
                                        scheduler.push_request_force(FrontierRequest::new(
                                            queued.request,
                                            queued.depth,
                                            queued.retry_count + 1,
                                        )).await;
                                    }
                                }
                                ErrorPolicy::Retry(_) => {
                                    tracing::warn!(spider = spider.name(), url = %url, error = %e, "fetch.skip.retry_exhausted");
                                }
                                ErrorPolicy::Skip => {
                                    tracing::warn!(spider = spider.name(), url = %url, error = %e, "fetch.skip");
                                }
                            }
                        }
                        Some(Err(join_err)) => {
                            error!(error = %join_err, "crawl task panicked");
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
        let elapsed = start.elapsed();

        for (i, (spider, _)) in spider_entries.iter().enumerate() {
            stats_vec[i].duration = elapsed;
            if let Err(e) = spider.close(&stats_vec[i]).await {
                tracing::error!(spider = spider.name(), error = %e, "spider::close failed");
            }
            let rps = if elapsed.as_secs_f64() > 0.0 {
                stats_vec[i].pages_crawled as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };
            info!(
                spider = spider.name(),
                pages = stats_vec[i].pages_crawled,
                items = stats_vec[i].items_scraped,
                errors = stats_vec[i].errors,
                bytes = stats_vec[i].bytes_downloaded,
                pages_per_sec = format!("{rps:.1}"),
                "spider complete"
            );
        }

        Ok(stats_vec)
    }
}
