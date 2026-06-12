use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use kumo::{
    extract::Response,
    frontier::{Frontier, MemoryFrontier},
    request::{CrawlRequest, FrontierRequest},
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

    c.bench_function("memory_frontier_push_pop_100", |b| {
        b.to_async(&runtime).iter(|| async {
            let frontier = MemoryFrontier::new(1_000);
            for i in 0..100 {
                let url = format!("https://example.com/catalogue/page-{i}.html");
                frontier.push_request(CrawlRequest::get(url), 0).await;
            }
            for _ in 0..100 {
                let _ = frontier.pop_request().await.unwrap();
            }
        })
    });
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

    c.bench_function("response_css_repeated_queries", |b| {
        b.iter(|| {
            let response = Response::from_parts("https://example.com", 200, html.clone());
            black_box(response.css("article.product_pod"));
            black_box(response.css("li.next a"));
        })
    });

    let response = Response::from_parts("https://example.com", 200, html);
    let products = response.css("article.product_pod");
    c.bench_function("element_nested_css_text_attr", |b| {
        b.iter(|| {
            for product in products.iter() {
                let title = product.css("h3 a");
                black_box(title.first().and_then(|element| element.attr("title")));
                let price = product.css(".price_color");
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
    bench_document_backed_selectors
);
criterion_main!(benches);
