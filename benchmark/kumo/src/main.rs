use kumo::prelude::*;
use kumo::store::ItemStore;
use serde::Serialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum StoreMode {
    Jsonl,
    Noop,
}

impl StoreMode {
    fn from_env() -> Result<Self, KumoError> {
        match std::env::var("STORE_MODE").as_deref() {
            Ok("noop") => Ok(Self::Noop),
            Ok("jsonl") | Err(std::env::VarError::NotPresent) => Ok(Self::Jsonl),
            Ok(value) => Err(KumoError::store_msg(format!(
                "unsupported benchmark STORE_MODE '{value}'; expected 'jsonl' or 'noop'"
            ))),
            Err(error) => Err(KumoError::store_msg(format!(
                "read benchmark STORE_MODE: {error}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Jsonl => "jsonl",
            Self::Noop => "noop",
        }
    }
}

struct NoopStore;

#[async_trait::async_trait]
impl ItemStore for NoopStore {
    async fn store(&self, _item: &serde_json::Value) -> Result<(), KumoError> {
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct Book {
    title: String,
    price: String,
}

struct BooksSpider {
    start_urls: Vec<String>,
    extraction_operations: Arc<ExtractionOperations>,
    progress: Arc<CrawlProgress>,
    products: CssSelector,
    titles: CssSelector,
    prices: CssSelector,
    next_page: CssSelector,
}

struct CrawlProgress {
    started_at: Instant,
    items: AtomicU64,
    first_10k_elapsed_us: AtomicU64,
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

#[derive(Clone, Default)]
struct ConcurrencyProbe {
    state: Arc<ConcurrencyProbeState>,
}

#[derive(Default)]
struct ConcurrencyProbeState {
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl ConcurrencyProbe {
    fn peak(&self) -> usize {
        self.state.peak.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl Middleware for ConcurrencyProbe {
    async fn before_request(&self, _request: &mut FetchRequest) -> Result<(), KumoError> {
        let active = self.state.active.fetch_add(1, Ordering::Relaxed) + 1;
        self.state.peak.fetch_max(active, Ordering::Relaxed);
        Ok(())
    }

    async fn after_response(&self, _response: &mut Response) -> Result<(), KumoError> {
        self.state.active.fetch_sub(1, Ordering::Relaxed);
        Ok(())
    }
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
        let start_urls =
            std::env::var("TARGET_URLS")
                .ok()
                .map(|urls| {
                    urls.split(',')
                        .filter(|url| !url.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|urls| !urls.is_empty())
                .unwrap_or_else(|| {
                    vec![std::env::var("TARGET_URL").unwrap_or_else(|_| {
                        "https://books.toscrape.com/catalogue/page-1.html".into()
                    })]
                });
        Self {
            start_urls,
            extraction_operations: Arc::new(ExtractionOperations::default()),
            progress: Arc::new(CrawlProgress {
                started_at: Instant::now(),
                items: AtomicU64::new(0),
                first_10k_elapsed_us: AtomicU64::new(0),
            }),
            products: CssSelector::parse("article.product_pod").expect("valid product selector"),
            titles: CssSelector::parse("h3 a").expect("valid title selector"),
            prices: CssSelector::parse(".price_color").expect("valid price selector"),
            next_page: CssSelector::parse("li.next a").expect("valid next-page selector"),
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
        self.start_urls.clone()
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        self.extraction_operations
            .root_css
            .fetch_add(1, Ordering::Relaxed);
        let books: Vec<Book> = res
            .css_with(&self.products)
            .iter()
            .map(|el| {
                self.extraction_operations
                    .nested_css
                    .fetch_add(2, Ordering::Relaxed);
                let title_element = el.css_with(&self.titles);
                let price_element = el.css_with(&self.prices);

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
        let previous = self
            .progress
            .items
            .fetch_add(books.len() as u64, Ordering::Relaxed);
        if previous < 10_000 && previous + books.len() as u64 >= 10_000 {
            let elapsed_us = self.progress.started_at.elapsed().as_micros() as u64;
            let _ = self.progress.first_10k_elapsed_us.compare_exchange(
                0,
                elapsed_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }

        self.extraction_operations
            .root_css
            .fetch_add(1, Ordering::Relaxed);
        self.extraction_operations
            .attr
            .fetch_add(1, Ordering::Relaxed);
        let next = res
            .css_with(&self.next_page)
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
    let scale_mode = std::env::var("SCALE_MODE").is_ok_and(|value| value == "true");
    let soak_mode = std::env::var("SOAK_MODE").is_ok_and(|value| value == "true");
    let realistic_mode = std::env::var("REALISTIC_MODE").is_ok_and(|value| value == "true");
    let store_mode = StoreMode::from_env()?;

    let spider = BooksSpider::new();
    let extraction_operations = Arc::clone(&spider.extraction_operations);
    let progress = Arc::clone(&spider.progress);
    let concurrency_probe = ConcurrencyProbe::default();
    let mut engine = CrawlEngine::builder()
        .concurrency(concurrency)
        .respect_robots_txt(false);
    engine = match store_mode {
        StoreMode::Jsonl => engine.store(JsonlStore::new("/results/kumo.jsonl")?),
        StoreMode::Noop => engine.store(NoopStore),
    };
    if scale_mode || soak_mode || realistic_mode {
        engine = engine
            .politeness(PolitenessPolicy::new().per_domain_concurrency(concurrency))
            .middleware(concurrency_probe.clone());
    }
    if realistic_mode {
        engine = engine.middleware(StatusRetry::new()).retry_policy(
            RetryPolicy::new(2)
                .base_delay(Duration::from_millis(10))
                .max_delay(Duration::from_millis(100))
                .on_status(429)
                .on_status(503),
        );
    }
    let stats = engine.run(spider).await?;

    let elapsed = start.elapsed().as_secs_f64();
    let rss_kb = peak_rss_kb();
    let report = CrawlReport::from(stats.clone());
    let report_json = report.to_json_value();
    let first_10k_elapsed_us = progress.first_10k_elapsed_us.load(Ordering::Relaxed);
    let first_10k_elapsed_s =
        (first_10k_elapsed_us > 0).then(|| first_10k_elapsed_us as f64 / 1_000_000.0);

    let result = serde_json::json!({
        "framework": "kumo",
        "elapsed_s": (elapsed * 1000.0).round() / 1000.0,
        "items": stats.items_scraped,
        "pages": stats.pages_crawled,
        "errors": stats.errors,
        "retries": stats.retries,
        "retry_exhausted": stats.retry_exhausted,
        "bytes_downloaded": stats.bytes_downloaded,
        "peak_rss_kb": rss_kb,
        "concurrency": concurrency,
        "store_mode": store_mode.as_str(),
        "peak_in_flight": (scale_mode || realistic_mode).then(|| concurrency_probe.peak()),
        "first_10k_elapsed_s": first_10k_elapsed_s,
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
