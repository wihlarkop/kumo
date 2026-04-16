//! Scrapes all books from https://books.toscrape.com, following pagination.
//! Demonstrates: multi-page crawl, allowed_domains, max_depth, RateLimiter, retry, JsonStore.
//!
//! Run with: cargo run --example books

use kumo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Book {
    title: String,
    price: String,
    rating: String,
    url: String,
}

struct BooksSpider;

#[async_trait::async_trait]
impl Spider for BooksSpider {
    fn name(&self) -> &str {
        "books"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://books.toscrape.com/catalogue/page-1.html".into()]
    }

    fn allowed_domains(&self) -> Vec<&str> {
        vec!["books.toscrape.com"]
    }

    fn max_depth(&self) -> Option<usize> {
        Some(60) // site has 50 pages
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        // Extract books on this page.
        let books: Vec<Book> = res
            .css("article.product_pod")
            .iter()
            .map(|el| {
                let title = el
                    .css("h3 a")
                    .first()
                    .and_then(|a| a.attr("title"))
                    .unwrap_or_default();
                let price = el
                    .css(".price_color")
                    .first()
                    .map(|e| e.text())
                    .unwrap_or_default();
                let rating = el
                    .css(".star-rating")
                    .first()
                    .and_then(|e| e.attr("class"))
                    .and_then(|cls| cls.split_whitespace().nth(1).map(String::from))
                    .unwrap_or_default();
                let href = el
                    .css("h3 a")
                    .first()
                    .and_then(|a| a.attr("href"))
                    .unwrap_or_default();
                Book {
                    title,
                    price,
                    rating,
                    url: res.urljoin(&href),
                }
            })
            .collect();

        // Follow the "next" pagination link if present.
        let next_url = res
            .css("li.next a")
            .first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(books);
        if let Some(url) = next_url {
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
        .concurrency(3)
        .middleware(RateLimiter::per_second(2.0))
        .middleware(DefaultHeaders::new().user_agent("kumo/0.1"))
        .store(JsonStore::new("books.json"))
        .crawl_delay(std::time::Duration::from_millis(300))
        .retry(2, std::time::Duration::from_millis(500))
        .respect_robots_txt(true)
        .run(BooksSpider)
        .await?;

    println!(
        "Done — scraped {} books from {} pages ({} errors)",
        stats.items_scraped, stats.pages_crawled, stats.errors
    );
    Ok(())
}
