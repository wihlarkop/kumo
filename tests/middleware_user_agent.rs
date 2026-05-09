use kumo::middleware::{FetchRequest, Middleware, UserAgentRotator};
use reqwest::header::USER_AGENT;

fn make_request() -> FetchRequest {
    FetchRequest::new("https://example.com", 0)
}

#[tokio::test]
async fn default_uses_common_browsers() {
    let rotator = UserAgentRotator::default();
    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    let ua = req.headers[USER_AGENT].to_str().unwrap();
    assert!(
        ua.contains("Mozilla"),
        "default UA should be a browser UA, got: {ua}"
    );
}

#[tokio::test]
async fn round_robin_cycles_in_order() {
    let rotator = UserAgentRotator::new(vec!["ua-a", "ua-b", "ua-c"]);
    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.headers[USER_AGENT], "ua-a");
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.headers[USER_AGENT], "ua-b");
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.headers[USER_AGENT], "ua-c");
    rotator.before_request(&mut req).await.unwrap();
    assert_eq!(req.headers[USER_AGENT], "ua-a");
}

#[tokio::test]
async fn random_picks_from_set() {
    let agents = vec!["ua-x", "ua-y", "ua-z"];
    let rotator = UserAgentRotator::random(agents.clone());
    for _ in 0..20 {
        let mut req = make_request();
        rotator.before_request(&mut req).await.unwrap();
        let picked = req.headers[USER_AGENT].to_str().unwrap().to_string();
        assert!(agents.contains(&picked.as_str()), "unexpected UA: {picked}");
    }
}

#[tokio::test]
async fn common_browsers_sets_header() {
    let rotator = UserAgentRotator::common_browsers();
    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    let ua = req.headers[USER_AGENT].to_str().unwrap();
    assert!(ua.contains("Mozilla"), "expected browser UA, got: {ua}");
}

#[tokio::test]
async fn empty_list_does_not_set_header() {
    let rotator = UserAgentRotator::new(Vec::<String>::new());
    let mut req = make_request();
    rotator.before_request(&mut req).await.unwrap();
    assert!(!req.headers.contains_key(USER_AGENT));
}
