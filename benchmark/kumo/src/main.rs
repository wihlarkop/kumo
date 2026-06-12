use kumo::prelude::*;
use serde::Serialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[derive(Debug, Serialize)]
struct Book {
    title: String,
    price: String,
}

struct BooksSpider {
    start_url: String,
    extraction_operations: Arc<ExtractionOperations>,
}

#[derive(Default)]
struct ExtractionOperations {
    root_css: AtomicU64,
    nested_css: AtomicU64,
    text: AtomicU64,
    attr: AtomicU64,
}

#[derive(Serialize)]
struct ExtractionOperationCounts {
    root_css: u64,
    nested_css: u64,
    text: u64,
    attr: u64,
}

impl ExtractionOperations {
    fn snapshot(&self) -> ExtractionOperationCounts {
        ExtractionOperationCounts {
            root_css: self.root_css.load(Ordering::Relaxed),
            nested_css: self.nested_css.load(Ordering::Relaxed),
            text: self.text.load(Ordering::Relaxed),
            attr: self.attr.load(Ordering::Relaxed),
        }
    }
}

impl BooksSpider {
    fn new() -> Self {
        Self {
            start_url: std::env::var("TARGET_URL")
                .unwrap_or_else(|_| "https://books.toscrape.com/catalogue/page-1.html".into()),
            extraction_operations: Arc::new(ExtractionOperations::default()),
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
        self.extraction_operations
            .root_css
            .fetch_add(1, Ordering::Relaxed);
        let books: Vec<Book> = res
            .css("article.product_pod")
            .iter()
            .map(|el| {
                self.extraction_operations
                    .nested_css
                    .fetch_add(2, Ordering::Relaxed);
                let title_element = el.css("h3 a");
                let price_element = el.css(".price_color");

                self.extraction_operations
                    .attr
                    .fetch_add(1, Ordering::Relaxed);
                let title = title_element
                    .first()
                    .and_then(|a| a.attr("title"))
                    .unwrap_or_default();

                self.extraction_operations
                    .text
                    .fetch_add(1, Ordering::Relaxed);
                let price = price_element.first().map(|e| e.text()).unwrap_or_default();

                Book { title, price }
            })
            .collect();

        self.extraction_operations
            .root_css
            .fetch_add(1, Ordering::Relaxed);
        self.extraction_operations
            .attr
            .fetch_add(1, Ordering::Relaxed);
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
    let concurrency: usize = std::env::var("CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16);

    let spider = BooksSpider::new();
    let extraction_operations = Arc::clone(&spider.extraction_operations);
    let stats = CrawlEngine::builder()
        .concurrency(concurrency)
        .respect_robots_txt(false)
        .store(JsonlStore::new("/results/kumo.jsonl")?)
        .run(spider)
        .await?;

    let elapsed = start.elapsed().as_secs_f64();
    let rss_kb = peak_rss_kb();
    let report = CrawlReport::from(stats.clone());
    let report_json = report.to_json_value();

    let result = serde_json::json!({
        "framework": "kumo",
        "elapsed_s": (elapsed * 1000.0).round() / 1000.0,
        "items": stats.items_scraped,
        "pages": stats.pages_crawled,
        "peak_rss_kb": rss_kb,
        "concurrency": concurrency,
        "timings": report_json["timings"].clone(),
        "extraction_operations": extraction_operations.snapshot(),
        "versions": {
            "language": format!(
                "rust {}",
                std::env::var("KUMO_BENCH_RUST_VERSION").unwrap_or_else(|_| "unknown".into())
            ),
            "framework": format!(
                "kumo {}",
                std::env::var("KUMO_BENCH_KUMO_VERSION")
                    .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())
            ),
        },
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
