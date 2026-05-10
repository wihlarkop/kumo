use criterion::{Criterion, criterion_group, criterion_main};
use kumo::prelude::FingerprintPolicy;

fn bench_fingerprint(c: &mut Criterion) {
    let policy = FingerprintPolicy::default().strip_tracking_params(true);

    c.bench_function("fingerprint_tracking_url", |b| {
        b.iter(|| {
            policy
                .fingerprint("https://Example.com/products/1?utm_source=x&id=1#details")
                .unwrap()
        })
    });
}

criterion_group!(benches, bench_fingerprint);
criterion_main!(benches);
