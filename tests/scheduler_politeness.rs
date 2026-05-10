use std::time::Duration;

use kumo::scheduler::PolitenessPolicy;

#[test]
fn politeness_policy_defaults_are_conservative_but_non_blocking() {
    let policy = PolitenessPolicy::default();

    assert_eq!(policy.default_per_domain_concurrency(), 8);
    assert_eq!(policy.default_per_domain_delay(), None);
    assert_eq!(policy.jitter_range(), None);
    assert!(policy.respects_robots_crawl_delay());
}

#[test]
fn politeness_policy_builder_sets_values() {
    let policy = PolitenessPolicy::new()
        .per_domain_concurrency(2)
        .per_domain_delay(Duration::from_millis(500))
        .jitter(Duration::from_millis(100))
        .respect_robots_crawl_delay(false);

    assert_eq!(policy.default_per_domain_concurrency(), 2);
    assert_eq!(
        policy.default_per_domain_delay(),
        Some(Duration::from_millis(500))
    );
    assert_eq!(policy.jitter_range(), Some(Duration::from_millis(100)));
    assert!(!policy.respects_robots_crawl_delay());
}
