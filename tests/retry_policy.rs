use std::time::Duration;

use kumo::{error::KumoError, retry::RetryPolicy};

#[test]
fn retry_policy_reports_deterministic_backoff_without_jitter() {
    let policy = RetryPolicy::new(4)
        .base_delay(Duration::from_millis(100))
        .max_delay(Duration::from_millis(250));

    assert_eq!(policy.max_attempts(), 4);
    assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
    assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
    assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(250));
    assert_eq!(policy.delay_for_attempt(20), Duration::from_millis(250));
}

#[test]
fn retry_policy_reports_jitter_bounds_without_sampling_randomness() {
    let policy = RetryPolicy::new(3)
        .base_delay(Duration::from_millis(200))
        .max_delay(Duration::from_millis(1_000))
        .jitter(true);

    assert_eq!(
        policy.delay_bounds_for_attempt(1),
        (Duration::from_millis(400), Duration::from_millis(500))
    );
    assert_eq!(
        policy.delay_bounds_for_attempt(3),
        (Duration::from_millis(1_000), Duration::from_millis(1_000))
    );
}

#[test]
fn retry_policy_classifies_retriable_errors_without_string_matching() {
    let all_transient = RetryPolicy::new(2);
    assert!(all_transient.is_retriable_error(&KumoError::http_status(429, "https://example.com")));

    let status_filtered = RetryPolicy::new(2).on_status(503);
    assert!(
        status_filtered.is_retriable_error(&KumoError::http_status(503, "https://example.com"))
    );
    assert!(
        !status_filtered.is_retriable_error(&KumoError::http_status(429, "https://example.com"))
    );
    assert!(!status_filtered.is_retriable_error(&KumoError::parse_msg("selector failed")));
}
