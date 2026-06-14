use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kumo::{
    extract::{CssSelector, Response},
    frontier::{Frontier, MemoryFrontier},
    request::{CrawlRequest, FrontierRequest},
    scheduler::{CrawlScheduler, PolitenessPolicy},
};

fn bench_domain_key(c: &mut Criterion) {
    c.bench_function("domain_key_valid_url", |b| {
        b.iter(|| domain_key("https://Example.com/catalogue/page-1.html?utm_source=x"))
    });

    let urls = (0..100)
        .map(|i| format!("https://Example.com/catalogue/page-{i}.html?utm_source=x"))
        .collect::<Vec<_>>();
    c.bench_function("domain_key_many_same_domain", |b| {
        b.iter(|| {
            for url in &urls {
                let _ = domain_key(url);
            }
        })
    });
}

fn bench_frontier_request_clone(c: &mut Criterion) {
    let request = FrontierRequest::new(
        CrawlRequest::get("https://example.com/catalogue/page-1.html"),
        3,
        1,
    );

    c.bench_function("frontier_request_clone", |b| b.iter(|| request.clone()));
}

fn bench_memory_frontier_batch(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("memory_frontier");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));

    for size in [100usize, 10_000] {
        group.bench_with_input(BenchmarkId::new("push_pop", size), &size, |b, &size| {
            b.to_async(&runtime).iter(|| async {
                let frontier = MemoryFrontier::new(size + 1);
                for i in 0..size {
                    let url = format!("https://example.com/catalogue/page-{i}.html");
                    frontier
                        .push_request(CrawlRequest::get(url).dont_filter(true), 0)
                        .await;
                }
                for _ in 0..size {
                    let _ = frontier.pop_request().await.unwrap();
                }
            })
        });
    }
    group.finish();
}

fn bench_crawl_scheduler_batch(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("crawl_scheduler");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));

    for size in [100usize, 10_000] {
        group.bench_with_input(
            BenchmarkId::new("push_dispatch_finish", size),
            &size,
            |b, &size| {
                b.to_async(&runtime).iter(|| async {
                    let scheduler = CrawlScheduler::new(
                        MemoryFrontier::new(size + 1),
                        PolitenessPolicy::default(),
                    );
                    for i in 0..size {
                        let url = format!("https://example.com/catalogue/page-{i}.html");
                        scheduler
                            .push_request(CrawlRequest::get(url).dont_filter(true), 0)
                            .await;
                    }
                    for _ in 0..size {
                        let queued = scheduler.next_ready().await.unwrap();
                        scheduler.finish(&queued).await;
                    }
                })
            },
        );
    }
    group.finish();
}

fn selector_fixture() -> String {
    let mut html = String::from("<html><body>");
    for index in 0..20 {
        html.push_str(&format!(
            r#"<article class="product_pod"><h3><a title="Book {index}">Book {index}</a></h3><p class="price_color">${index}.00</p></article>"#
        ));
    }
    html.push_str(r#"<li class="next"><a href="page-2.html">next</a></li></body></html>"#);
    html
}

fn bench_document_backed_selectors(c: &mut Criterion) {
    let html = selector_fixture();

    c.bench_function("response_document_parse", |b| {
        b.iter(|| {
            let response = Response::from_parts("https://example.com", 200, html.clone());
            black_box(response.css("article.product_pod"));
        })
    });

    let response = Response::from_parts("https://example.com", 200, html);
    black_box(response.css("article.product_pod"));
    c.bench_function("response_root_css_cached", |b| {
        b.iter(|| black_box(response.css("article.product_pod")))
    });
    let product_selector =
        CssSelector::parse("article.product_pod").expect("valid product selector");
    c.bench_function("response_root_css_compiled", |b| {
        b.iter(|| black_box(response.css_with(&product_selector)))
    });

    let products = response.css("article.product_pod");
    let product = products.first().expect("fixture must contain a product");
    c.bench_function("element_nested_css_cached", |b| {
        b.iter(|| black_box(product.css("h3 a")))
    });
    let title_selector = CssSelector::parse("h3 a").expect("valid title selector");
    c.bench_function("element_nested_css_compiled", |b| {
        b.iter(|| black_box(product.css_with(&title_selector)))
    });

    let title_elements = product.css("h3 a");
    let title = title_elements
        .first()
        .expect("fixture product must contain a title");
    c.bench_function("element_attr_copy", |b| {
        b.iter(|| black_box(title.attr("title")))
    });

    let price_elements = product.css(".price_color");
    let price = price_elements
        .first()
        .expect("fixture product must contain a price");
    c.bench_function("element_text_collect", |b| {
        b.iter(|| black_box(price.text()))
    });

    c.bench_function("page_extraction_workload", |b| {
        b.iter(|| {
            for product in products.iter() {
                let title = product.css("h3 a");
                black_box(title.first().and_then(|element| element.attr("title")));
                let price = product.css(".price_color");
                black_box(price.first().map(|element| element.text()));
            }
        })
    });

    let price_selector = CssSelector::parse(".price_color").expect("valid price selector");
    c.bench_function("page_extraction_workload_compiled", |b| {
        b.iter(|| {
            for product in products.iter() {
                let title = product.css_with(&title_selector);
                black_box(title.first().and_then(|element| element.attr("title")));
                let price = product.css_with(&price_selector);
                black_box(price.first().map(|element| element.text()));
            }
        })
    });
}

fn domain_key(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_else(|| "<unknown>".to_string())
}

criterion_group!(
    benches,
    bench_domain_key,
    bench_frontier_request_clone,
    bench_memory_frontier_batch,
    bench_crawl_scheduler_batch,
    bench_document_backed_selectors
);
criterion_main!(benches);
