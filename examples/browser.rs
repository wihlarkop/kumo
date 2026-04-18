/// Demonstrates Kumo's headless browser fetcher against a JavaScript-rendered page.
///
/// `quotes.toscrape.com/js/` renders its quotes via JavaScript — plain HTTP
/// returns an empty list. The browser fetcher waits for the `.quote` selector
/// before reading the page, so all 10 quotes are captured.
///
/// Run with:
///     cargo run --example browser --features browser
use kumo::prelude::*;

struct QuotesJsSpider;

#[async_trait::async_trait]
impl Spider for QuotesJsSpider {
    fn name(&self) -> &str {
        "quotes-js"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com/js/".to_string()]
    }

    fn allowed_domains(&self) -> Vec<&str> {
        vec!["quotes.toscrape.com"]
    }

    async fn parse(&self, response: Response) -> Result<Output, KumoError> {
        let mut output = Output::new();

        for quote in response.css(".quote").iter() {
            let text = quote
                .css(".text")
                .first()
                .map(|e| e.text())
                .unwrap_or_default();
            let author = quote
                .css(".author")
                .first()
                .map(|e| e.text())
                .unwrap_or_default();

            output = output.item(serde_json::json!({
                "text": text,
                "author": author,
            }));
        }

        // Follow the "Next" button if present.
        if let Some(href) = response
            .css("li.next a")
            .first()
            .and_then(|e| e.attr("href"))
        {
            output = output.follow(response.urljoin(&href));
        }

        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();

    let stats = CrawlEngine::builder()
        .concurrency(2)
        .browser(
            BrowserConfig::headless()
                .wait_for_selector(".quote")
                .viewport(1280, 800),
        )
        .store(StdoutStore)
        .run(QuotesJsSpider)
        .await?;

    println!(
        "\nCrawled {} page(s), scraped {} quote(s) in {:.2}s",
        stats.pages_crawled,
        stats.items_scraped,
        stats.duration.as_secs_f64()
    );

    Ok(())
}
