use kumo::prelude::*;
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Serialize)]
struct Book {
    title: String,
    price: String,
}

struct BooksSpider {
    start_url: String,
}

impl BooksSpider {
    fn new() -> Self {
        Self {
            start_url: std::env::var("TARGET_URL")
                .unwrap_or_else(|_| {
                    "https://books.toscrape.com/catalogue/page-1.html".into()
                }),
        }
    }
}

#[async_trait::async_trait]
impl Spider for BooksSpider {
    type Item = Book;

    fn name(&self) -> &str {
        "books"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.start_url.clone()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let books: Vec<Book> = res
            .css("article.product_pod")
            .iter()
            .map(|el| Book {
                title: el
                    .css("h3 a")
                    .first()
                    .and_then(|a| a.attr("title"))
                    .unwrap_or_default(),
                price: el
                    .css(".price_color")
                    .first()
                    .map(|e| e.text())
                    .unwrap_or_default(),
            })
            .collect();

        let next = res
            .css("li.next a")
            .first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(books);
        if let Some(url) = next {
            output = output.follow(url);
        }
        Ok(output)
    }
}

fn peak_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    let start = Instant::now();

    let stats = CrawlEngine::builder()
        .concurrency(16)
        .respect_robots_txt(false)
        .store(JsonlStore::new("/results/kumo.jsonl")?)
        .run(BooksSpider::new())
        .await?;

    let elapsed = start.elapsed().as_secs_f64();
    let rss_kb = peak_rss_kb();

    let result = serde_json::json!({
        "elapsed_s": (elapsed * 1000.0).round() / 1000.0,
        "items": stats.items_scraped,
        "pages": stats.pages_crawled,
        "peak_rss_kb": rss_kb,
    });
    std::fs::write("/results/kumo_stats.json", result.to_string()).ok();

    eprintln!(
        "kumo: {} items in {:.2}s ({:.1} items/s, {:.1} MB peak RSS)",
        stats.items_scraped,
        elapsed,
        stats.items_scraped as f64 / elapsed,
        rss_kb as f64 / 1024.0
    );
    Ok(())
}
