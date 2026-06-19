use std::time::Duration;

use kumo::{
    error::KumoError,
    extract::Response,
    middleware::{FetchRequest, Middleware, ProxyCircuitState, ProxyRotator},
};

fn make_request() -> FetchRequest {
    FetchRequest::new("https://example.com", 0)
}

#[tokio::test]
async fn round_robin_assigns_proxies_in_order() {
    let rotator = ProxyRotator::new(vec!["http://p1:8080", "http://p2:8080"]);
    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.proxy.as_deref(), Some("http://p1:8080"));
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.proxy.as_deref(), Some("http://p2:8080"));
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.proxy.as_deref(), Some("http://p1:8080"));
}

#[tokio::test]
async fn random_picks_from_pool() {
    let proxies = vec!["http://p1:8080", "http://p2:8080", "http://p3:8080"];
    let rotator = ProxyRotator::random(proxies.clone());
    for _ in 0..30 {
        let mut req = make_request();
        rotator.before_request(&mut req).await.unwrap();
        let picked = req.proxy.unwrap();
        assert!(
            proxies.contains(&picked.as_str()),
            "unexpected proxy: {picked}"
        );
    }
}

#[tokio::test]
async fn empty_pool_leaves_proxy_none() {
    let rotator = ProxyRotator::new(Vec::<String>::new());
    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    assert!(req.proxy.is_none());
}

#[tokio::test]
async fn records_proxy_successes_and_failures() {
    let rotator = ProxyRotator::new(vec!["http://p1:8080"]);
    let mut req = make_request();

    rotator.before_request(&mut req).await.unwrap();
    rotator
        .after_response(&mut Response::from_parts("https://example.com", 200, "ok"))
        .await
        .unwrap();

    rotator.before_request(&mut req).await.unwrap();
    rotator
        .on_error(
            "https://example.com",
            &KumoError::http_status(503, "https://example.com"),
        )
        .await;

    let snapshot = rotator.health();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].proxy, "http://p1:8080");
    assert_eq!(snapshot[0].successes, 1);
    assert_eq!(snapshot[0].failures, 1);
    assert_eq!(snapshot[0].consecutive_failures, 1);
    assert!(!snapshot[0].cooling_down);
}

#[tokio::test]
async fn skips_proxy_while_cooling_down_after_repeated_failures() {
    let rotator = ProxyRotator::new(vec!["http://p1:8080", "http://p2:8080"])
        .cooldown_after(1, Duration::from_secs(60));

    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.proxy.as_deref(), Some("http://p1:8080"));
    rotator
        .on_error(
            "https://example.com",
            &KumoError::http_status(503, "https://example.com"),
        )
        .await;

    let mut next = make_request();
    rotator.before_request(&mut next).await.unwrap();
    assert_eq!(next.proxy.as_deref(), Some("http://p2:8080"));
}

#[tokio::test]
async fn leaves_proxy_unset_when_all_proxies_are_cooling_down() {
    let rotator =
        ProxyRotator::new(vec!["http://p1:8080"]).cooldown_after(1, Duration::from_secs(60));

    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    rotator
        .on_error(
            "https://example.com",
            &KumoError::http_status(503, "https://example.com"),
        )
        .await;

    let mut next = make_request();
    rotator.before_request(&mut next).await.unwrap();
    assert!(next.proxy.is_none());
}

#[tokio::test]
async fn reports_open_recovering_and_healthy_circuit_states() {
    let rotator =
        ProxyRotator::new(vec!["http://p1:8080"]).cooldown_after(1, Duration::from_millis(5));

    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    rotator
        .on_error(
            "https://example.com",
            &KumoError::http_status(503, "https://example.com"),
        )
        .await;

    let open = rotator.health();
    assert_eq!(open[0].circuit_state, ProxyCircuitState::Open);
    assert!(open[0].cooling_down);

    tokio::time::sleep(Duration::from_millis(10)).await;

    let recovering = rotator.health();
    assert_eq!(recovering[0].circuit_state, ProxyCircuitState::Recovering);
    assert!(!recovering[0].cooling_down);

    let mut retry = make_request();
    rotator.before_request(&mut retry).await.unwrap();
    assert_eq!(retry.proxy.as_deref(), Some("http://p1:8080"));
    rotator
        .after_response(&mut Response::from_parts("https://example.com", 200, "ok"))
        .await
        .unwrap();

    let healthy = rotator.health();
    assert_eq!(healthy[0].circuit_state, ProxyCircuitState::Healthy);
    assert_eq!(healthy[0].consecutive_failures, 0);
}

#[tokio::test]
async fn cloned_rotators_share_health_snapshots() {
    let rotator = ProxyRotator::new(vec!["http://p1:8080"]);
    let handle = rotator.clone();

    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    rotator
        .after_response(&mut Response::from_parts("https://example.com", 200, "ok"))
        .await
        .unwrap();

    assert_eq!(handle.health()[0].successes, 1);
}
