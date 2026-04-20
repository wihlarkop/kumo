//! Demonstrates XPath 1.0 selectors for element, text, and attribute extraction.
//!
//! Run with: cargo run --example xpath --features xpath

use kumo::prelude::*;

struct QuotesSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "quotes-xpath"
    }
    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }
    fn max_depth(&self) -> Option<usize> {
        Some(1)
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        // XPath equivalents of CSS selectors — returns Vec<String> directly:
        //   CSS:   res.css(".quote .text").iter().map(|e| e.text())
        //   XPath: res.xpath(r#"//span[@class="text"]/text()"#)
        let texts = res.xpath(r#"//span[@class="text"]/text()"#);
        let authors = res.xpath(r#"//small[@class="author"]/text()"#);

        let items: Vec<serde_json::Value> = texts
            .into_iter()
            .zip(authors)
            .map(|(text, author)| serde_json::json!({"text": text, "author": author}))
            .collect();

        // Attribute extraction: get href from the Next button
        let next = res
            .xpath_first(r#"//li[@class="next"]/a/@href"#)
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(items);
        if let Some(url) = next {
            output = output.follow(url);
        }
        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();
    let stats = CrawlEngine::builder()
        .concurrency(2)
        .store(StdoutStore)
        .run(QuotesSpider)
        .await?;
    println!(
        "Scraped {} quotes from {} pages",
        stats.items_scraped, stats.pages_crawled
    );
    Ok(())
}
