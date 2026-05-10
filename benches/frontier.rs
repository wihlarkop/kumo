use criterion::{Criterion, criterion_group, criterion_main};
use kumo::{frontier::Frontier, frontier::MemoryFrontier, request::CrawlRequest};

fn bench_memory_frontier_push_pop(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("memory_frontier_push_pop", |b| {
        b.to_async(&runtime).iter(|| async {
            let frontier = MemoryFrontier::new(10_000);
            frontier
                .push_request(CrawlRequest::get("https://example.com/a"), 0)
                .await;
            frontier.pop_request().await.unwrap();
        })
    });
}

criterion_group!(benches, bench_memory_frontier_push_pop);
criterion_main!(benches);
