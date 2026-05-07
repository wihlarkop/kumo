use kumo::middleware::{Middleware, ProxyRotator, Request};

fn make_request() -> Request {
    Request::new("https://example.com", 0)
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
