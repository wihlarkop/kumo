use criterion::{Criterion, criterion_group, criterion_main};
use kumo::{
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
    bench_memory_frontier_batch
);
criterion_main!(benches);
