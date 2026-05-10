use std::time::Duration;

use kumo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Page {
    url: String,
    title: String,
}

struct PoliteSpider;

#[async_trait::async_trait]
impl Spider for PoliteSpider {
    type Item = Page;

    fn name(&self) -> &str {
        "polite"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".to_string()]
    }

    fn allowed_domains(&self) -> Vec<&str> {
        vec!["quotes.toscrape.com"]
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        let title = response
            .css("title")
            .first()
            .map(|title| title.text())
            .unwrap_or_default();

        let mut output = Output::new().item(Page {
            url: response.url().to_string(),
            title,
        });

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
    let stats = CrawlEngine::builder()
        .concurrency(16)
        .politeness(
            PolitenessPolicy::new()
                .per_domain_concurrency(2)
                .per_domain_delay(Duration::from_millis(500)),
        )
        .fingerprint_policy(FingerprintPolicy::default().strip_tracking_params(true))
        .store(JsonlStore::new("polite-pages.jsonl")?)
        .run(PoliteSpider)
        .await?;

    println!(
        "crawled {} pages, scheduled {}, deduped {}",
        stats.pages_crawled, stats.scheduled, stats.deduped
    );
    Ok(())
}
