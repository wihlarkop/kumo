//! Same as books.rs but uses #[derive(Extract)] to eliminate manual field extraction.
//! Run with: cargo run --example books_derive --features derive

use kumo::prelude::*;
use serde::Serialize;

#[derive(ExtractDerive, Serialize)]
struct Book {
    #[extract(css = "h3 a", attr = "title")]
    title: String,
    #[extract(css = ".price_color")]
    price: String,
    #[extract(css = ".star-rating", attr = "class")]
    rating: Option<String>,
    #[extract(css = "h3 a", attr = "href")]
    href: String,
}

struct BooksSpider;

#[async_trait::async_trait]
impl Spider for BooksSpider {
    type Item = Book;
    fn name(&self) -> &str {
        "books-derive"
    }
    fn start_urls(&self) -> Vec<String> {
        vec!["https://books.toscrape.com/catalogue/page-1.html".into()]
    }
    fn allowed_domains(&self) -> Vec<&str> {
        vec!["books.toscrape.com"]
    }
    fn max_depth(&self) -> Option<usize> {
        Some(60)
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let mut books = Vec::new();
        for el in res.css("article.product_pod").iter() {
            books.push(Book::extract_from(el, None).await?);
        }

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
        .store(JsonlStore::new("books_derive.jsonl")?)
        .run(BooksSpider)
        .await?;
    println!(
        "Scraped {} books from {} pages",
        stats.items_scraped, stats.pages_crawled
    );
    Ok(())
}
