use std::time::Duration;

use kumo::FileFrontier;
use kumo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Quote {
    url: String,
    text: String,
    author: String,
}

struct ProductionSpider;

#[async_trait::async_trait]
impl Spider for ProductionSpider {
    type Item = Quote;

    fn name(&self) -> &str {
        "production-quotes"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".to_string()]
    }

    fn allowed_domains(&self) -> Vec<&str> {
        vec!["quotes.toscrape.com"]
    }

    fn max_depth(&self) -> Option<usize> {
        Some(10)
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        let quotes: Vec<Quote> = response
            .css(".quote")
            .iter()
            .map(|quote| Quote {
                url: response.url().to_string(),
                text: quote
                    .css(".text")
                    .first()
                    .map(|text| text.text())
                    .unwrap_or_default(),
                author: quote
                    .css(".author")
                    .first()
                    .map(|author| author.text())
                    .unwrap_or_default(),
            })
            .collect();

        let mut output = Output::new().items(quotes);

        if let Some(next) = response
            .css("li.next a")
            .first()
            .and_then(|link| link.attr("href"))
            .map(|href| response.urljoin(&href))
        {
            output = output.request(
                CrawlRequest::get(next)
                    .priority(10)
                    .meta("source", "pagination"),
            );
        }

        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "kumo=info,production_crawler=info".into()),
        )
        .init();

    let frontier = FileFrontier::open("production-frontier")?.flush_every(25);
    let state = frontier.state().await;
    println!(
        "frontier recovered {} queued requests and {} seen fingerprints",
        state.queued, state.seen
    );

    let stats = CrawlEngine::builder()
        .concurrency(16)
        .respect_robots_txt(true)
        .politeness(
            PolitenessPolicy::new()
                .per_domain_concurrency(2)
                .per_domain_delay(Duration::from_millis(500))
                .jitter(Duration::from_millis(250)),
        )
        .retry_policy(
            RetryPolicy::new(3)
                .base_delay(Duration::from_millis(250))
                .max_delay(Duration::from_secs(30))
                .jitter(true)
                .on_status(429)
                .on_status(500)
                .on_status(502)
                .on_status(503)
                .on_status(504),
        )
        .middleware(DefaultHeaders::new().user_agent("kumo-production-example/0.2"))
        .middleware(StatusRetry::new())
        .fingerprint_policy(FingerprintPolicy::default().strip_tracking_params(true))
        .frontier(frontier)
        .metrics_interval(Duration::from_secs(30))
        .store(JsonlStore::new("production-quotes.jsonl")?)
        .run(ProductionSpider)
        .await?;

    let report = CrawlReport::from(stats.clone());
    std::fs::write(
        "production-crawl-report.json",
        report.to_json_string_pretty(),
    )
    .map_err(|e| KumoError::store("write production crawl report", e))?;

    println!(
        "pages={} items={} scheduled={} errors={} error_kinds={:?} retries={} retry_exhausted={} deduped={} robots_blocked={}",
        stats.pages_crawled,
        stats.items_scraped,
        stats.scheduled,
        stats.errors,
        stats.error_kinds,
        stats.retries,
        stats.retry_exhausted,
        stats.deduped,
        stats.robots_blocked
    );

    Ok(())
}
