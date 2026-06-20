use std::{sync::Arc, time::Duration};

use tokio::sync::Barrier;

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
    let open_rotator =
        ProxyRotator::new(vec!["http://p1:8080"]).cooldown_after(1, Duration::from_secs(60));

    let mut req = make_request();
    open_rotator.before_request(&mut req).await.unwrap();
    open_rotator
        .on_error(
            "https://example.com",
            &KumoError::http_status(503, "https://example.com"),
        )
        .await;

    let open = open_rotator.circuit_health();
    assert_eq!(open[0].circuit_state, ProxyCircuitState::Open);
    assert!(open[0].cooling_down);

    let rotator = ProxyRotator::new(vec!["http://p1:8080"]).cooldown_after(1, Duration::ZERO);
    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    rotator
        .on_error(
            "https://example.com",
            &KumoError::http_status(503, "https://example.com"),
        )
        .await;

    let recovering = rotator.circuit_health();
    assert_eq!(recovering[0].circuit_state, ProxyCircuitState::Recovering);
    assert!(!recovering[0].cooling_down);

    let mut retry = make_request();
    rotator.before_request(&mut retry).await.unwrap();
    assert_eq!(retry.proxy.as_deref(), Some("http://p1:8080"));
    rotator
        .after_response(&mut Response::from_parts("https://example.com", 200, "ok"))
        .await
        .unwrap();

    let healthy = rotator.circuit_health();
    assert_eq!(healthy[0].circuit_state, ProxyCircuitState::Healthy);
    assert_eq!(healthy[0].consecutive_failures, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recovering_proxy_allows_only_one_concurrent_trial() {
    const ATTEMPTS: usize = 16;

    let rotator = ProxyRotator::new(vec!["http://p1:8080"]).cooldown_after(1, Duration::ZERO);
    let mut initial = FetchRequest::new("https://example.com/initial", 0);
    rotator.before_request(&mut initial).await.unwrap();
    rotator
        .on_error(initial.url(), &KumoError::http_status(503, initial.url()))
        .await;

    let barrier = Arc::new(Barrier::new(ATTEMPTS + 1));
    let mut attempts = Vec::with_capacity(ATTEMPTS);
    for index in 0..ATTEMPTS {
        let rotator = rotator.clone();
        let barrier = Arc::clone(&barrier);
        attempts.push(tokio::spawn(async move {
            let url = format!("https://example.com/trial/{index}");
            let mut request = FetchRequest::new(url.clone(), 0);
            barrier.wait().await;
            rotator.before_request(&mut request).await.unwrap();
            (url, request.proxy)
        }));
    }

    barrier.wait().await;
    let mut selected = Vec::new();
    for attempt in attempts {
        let (url, proxy) = attempt.await.unwrap();
        if proxy.is_some() {
            selected.push(url);
        }
    }

    assert_eq!(selected.len(), 1);

    let mut blocked = FetchRequest::new("https://example.com/blocked", 0);
    rotator.before_request(&mut blocked).await.unwrap();
    assert!(blocked.proxy.is_none());

    rotator
        .after_response(&mut Response::from_parts(&selected[0], 200, "ok"))
        .await
        .unwrap();
    assert_eq!(
        rotator.circuit_health()[0].circuit_state,
        ProxyCircuitState::Healthy
    );
}

#[tokio::test]
async fn stale_success_does_not_release_recovery_trial() {
    let rotator = ProxyRotator::new(vec!["http://p1:8080"]).cooldown_after(1, Duration::ZERO);

    let mut stale = FetchRequest::new("https://example.com/stale", 0);
    rotator.before_request(&mut stale).await.unwrap();

    let mut opener = FetchRequest::new("https://example.com/opener", 0);
    rotator.before_request(&mut opener).await.unwrap();
    rotator
        .on_error(opener.url(), &KumoError::http_status(503, opener.url()))
        .await;

    let mut trial = FetchRequest::new("https://example.com/trial", 0);
    rotator.before_request(&mut trial).await.unwrap();
    assert_eq!(trial.proxy.as_deref(), Some("http://p1:8080"));

    rotator
        .after_response(&mut Response::from_parts(stale.url(), 200, "ok"))
        .await
        .unwrap();

    let mut blocked = FetchRequest::new("https://example.com/blocked", 0);
    rotator.before_request(&mut blocked).await.unwrap();
    assert!(blocked.proxy.is_none());

    rotator
        .after_response(&mut Response::from_parts(trial.url(), 200, "ok"))
        .await
        .unwrap();
    assert_eq!(
        rotator.circuit_health()[0].circuit_state,
        ProxyCircuitState::Healthy
    );
}

#[tokio::test]
async fn failed_recovery_trial_reopens_for_cooldown() {
    let recovering = ProxyRotator::new(vec!["http://p1:8080"]).cooldown_after(1, Duration::ZERO);
    let mut initial = FetchRequest::new("https://example.com/initial", 0);
    recovering.before_request(&mut initial).await.unwrap();
    recovering
        .on_error(initial.url(), &KumoError::http_status(503, initial.url()))
        .await;

    let rotator = recovering.cooldown_after(1, Duration::from_secs(60));
    let mut trial = FetchRequest::new("https://example.com/trial", 0);
    rotator.before_request(&mut trial).await.unwrap();
    assert_eq!(trial.proxy.as_deref(), Some("http://p1:8080"));
    rotator
        .on_error(trial.url(), &KumoError::http_status(503, trial.url()))
        .await;

    let snapshot = rotator.circuit_health();
    assert_eq!(snapshot[0].circuit_state, ProxyCircuitState::Open);
    assert!(snapshot[0].cooling_down);

    let mut blocked = FetchRequest::new("https://example.com/blocked", 0);
    rotator.before_request(&mut blocked).await.unwrap();
    assert!(blocked.proxy.is_none());
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
