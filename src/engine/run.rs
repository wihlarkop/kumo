use std::sync::Arc;

use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    error::{ErrorPolicy, KumoError},
    frontier::{Frontier, memory::MemoryFrontier},
    middleware::Middleware,
    pipeline::Pipeline,
    request::{CrawlRequest, FrontierRequest},
    scheduler::CrawlScheduler,
    spider::Spider,
    stats::{CrawlStats, domain_key},
};

use super::{
    builder::CrawlEngine,
    erased::{ErasedSpider, SpiderErased},
    setup::{
        FetcherArgs, build_http_client, build_raw_fetcher, build_robots_cache, wrap_with_cache,
    },
    task::{TaskContext, is_cancelled, process_request_once, should_enqueue},
};

impl CrawlEngine {
    /// Consume the engine, run the spider, and return crawl statistics.
    pub async fn run<S>(self, spider: S) -> Result<CrawlStats, KumoError>
    where
        S: Spider + 'static,
    {
        let start = std::time::Instant::now();
        let metrics_interval = self.metrics_interval;
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
        info!(
            spider = spider.name(),
            start_urls = start_urls.len(),
            "spider.open"
        );
        for url in start_urls {
            let domain = domain_key(&url);
            if scheduler.push_request(CrawlRequest::get(url), 0).await {
                stats.record_scheduled(&domain);
            } else {
                stats.record_deduped(&domain);
            }
        }

        type TaskResult = (
            FrontierRequest,
            Result<(u64, u64, Vec<(CrawlRequest, usize)>), KumoError>,
        );
        let mut join_set: JoinSet<TaskResult> = JoinSet::new();

        // Spawn periodic metrics logger if configured.
        let live_stats = Arc::new(tokio::sync::Mutex::new(CrawlStats::default()));
        let _metrics_task = metrics_interval.map(|interval| {
            let live = live_stats.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(interval).await;
                    let s = live.lock().await;
                    tracing::info!(
                        pages = s.pages_crawled,
                        items = s.items_scraped,
                        errors = s.errors,
                        bytes = s.bytes_downloaded,
                        elapsed_secs = s.duration.as_secs_f64(),
                        "[kumo metrics]"
                    );
                }
            })
        });

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

        loop {
            if is_cancelled(&stream_cancelled) {
                shutting_down = true;
                stats.interrupted = true;
            }

            if !shutting_down {
                // Fill up to the concurrency limit.
                while join_set.len() < concurrency {
                    match scheduler.try_next_ready().await {
                        Some(queued) => {
                            // Check robots.txt before dispatching.
                            if let Some(ref cache) = robots_cache
                                && !cache.is_allowed(&client, queued.request.url()).await
                            {
                                tracing::debug!(url = %queued.request.url(), "blocked by robots.txt, skipping");
                                stats.record_robots_blocked(&domain_key(queued.request.url()));
                                scheduler.finish(&queued).await;
                                continue;
                            }

                            let ctx = TaskContext {
                                spider: spider.clone(),
                                store: store.clone(),
                                middleware: middleware.clone(),
                                pipelines: pipelines.clone(),
                                fetcher: fetcher.clone(),
                                stream_cancelled: stream_cancelled.clone(),
                            };

                            join_set.spawn(async move {
                                let result = process_request_once(queued.clone(), ctx).await;
                                (queued, result)
                            });
                        }
                        // Frontier currently empty â€” tasks may still add URLs.
                        None => break,
                    }
                }
            }

            if join_set.is_empty() {
                if scheduler.is_empty().await {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                continue;
            }

            tokio::select! {
                _ = &mut shutdown, if !shutting_down => {
                    shutting_down = true;
                    stats.interrupted = true;
                }
                result = join_set.join_next() => {
                    match result {
                        Some(Ok((queued, Ok((item_count, bytes, follows))))) => {
                            scheduler.finish(&queued).await;
                            stats.record_completed(&domain_key(queued.request.url()));
                            stats.pages_crawled += 1;
                            stats.items_scraped += item_count;
                            stats.bytes_downloaded += bytes;
                            if is_cancelled(&stream_cancelled) {
                                shutting_down = true;
                                stats.interrupted = true;
                            }
                            // Keep live snapshot up to date for the metrics task.
                            if metrics_interval.is_some() {
                                let mut snap = live_stats.lock().await;
                                *snap = stats.clone();
                                snap.duration = start.elapsed();
                            }

                            if !shutting_down {
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
                        Some(Ok((queued, Err(e)))) => {
                            scheduler.finish(&queued).await;
                            let url = queued.request.url().to_string();
                            // Notify all middleware of the permanent failure.
                            for mw in middleware.iter() {
                                mw.on_error(&url, &e).await;
                            }
                            if !shutting_down
                                && queued.retry_count < retry_policy.max_attempts
                                && retry_policy.is_retriable(&e)
                            {
                                let delay = retry_policy.delay_for(queued.retry_count);
                                stats.record_retry(&domain_key(&url));
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

                            stats.errors += 1;
                            stats.record_failed(&domain_key(&url));
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
                                        stats.record_retry(&domain_key(&url));
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
                            stats.errors += 1;
                            error!(spider = spider.name(), error = %join_err, "crawl task panicked");
                        }
                        None => break,
                    }

                    if shutting_down && join_set.is_empty() {
                        break;
                    }
                }
            }
        }

        store.flush().await?;
        stats.duration = start.elapsed();

        // close() errors are intentionally not propagated â€” the crawl and store
        // flush completed successfully. Cleanup failures are logged only.
        if let Err(e) = spider.close(&stats).await {
            tracing::error!(error = %e, "spider::close failed");
        }

        let rps = if stats.duration.as_secs_f64() > 0.0 {
            stats.pages_crawled as f64 / stats.duration.as_secs_f64()
        } else {
            0.0
        };
        info!(
            pages = stats.pages_crawled,
            items = stats.items_scraped,
            errors = stats.errors,
            bytes = stats.bytes_downloaded,
            duration_secs = stats.duration.as_secs_f64(),
            pages_per_sec = format!("{rps:.1}"),
            interrupted = stats.interrupted,
            "crawl complete"
        );

        Ok(stats)
    }
}
