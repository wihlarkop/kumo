use kumo::{
    error::KumoError,
    extract::Response,
    middleware::{Middleware, StatusRetry},
};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::time::{Duration, SystemTime};

fn make_response(url: &str, status: u16) -> Response {
    Response::from_parts(url, status, "")
}

fn make_response_with_retry_after(url: &str, status: u16, retry_after: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, retry_after.parse().unwrap());
    Response::from_parts_with_headers(url, status, headers, "")
}

#[tokio::test]
async fn allows_200() {
    let mw = StatusRetry::new();
    let mut res = make_response("https://example.com/page", 200);
    assert!(mw.after_response(&mut res).await.is_ok());
}

#[tokio::test]
async fn rejects_429() {
    let mw = StatusRetry::new();
    let mut res = make_response("https://example.com/page", 429);
    let err = mw.after_response(&mut res).await.unwrap_err();
    assert!(matches!(err, KumoError::HttpStatus { status: 429, .. }));
}

#[tokio::test]
async fn rejects_503() {
    let mw = StatusRetry::new();
    let mut res = make_response("https://example.com/page", 503);
    assert!(matches!(
        mw.after_response(&mut res).await.unwrap_err(),
        KumoError::HttpStatus { status: 503, .. }
    ));
}

#[tokio::test]
async fn custom_codes_respected() {
    let mw = StatusRetry::with_codes(vec![403]);
    let mut res = make_response("https://example.com/page", 403);
    assert!(mw.after_response(&mut res).await.is_err());
    let mut ok = make_response("https://example.com/page", 503);
    assert!(mw.after_response(&mut ok).await.is_ok());
}

#[tokio::test]
async fn pattern_overrides_global_for_matching_url() {
    let mw = StatusRetry::new().for_pattern(r"^https://example\.com/api/", vec![404]);
    let mut api_404 = make_response("https://example.com/api/users", 404);
    assert!(matches!(
        mw.after_response(&mut api_404).await.unwrap_err(),
        KumoError::HttpStatus { status: 404, .. }
    ));
}

#[tokio::test]
async fn pattern_opts_out_for_matching_url() {
    let mw = StatusRetry::new().for_pattern(r"\.(js|css|png)$", vec![]);
    let mut static_500 = make_response("https://example.com/style.css", 500);
    assert!(mw.after_response(&mut static_500).await.is_ok());
}

#[tokio::test]
async fn global_codes_apply_when_no_pattern_matches() {
    let mw = StatusRetry::new().for_pattern(r"^https://example\.com/api/", vec![404]);
    let mut other_500 = make_response("https://example.com/page", 500);
    assert!(matches!(
        mw.after_response(&mut other_500).await.unwrap_err(),
        KumoError::HttpStatus { status: 500, .. }
    ));
}

#[tokio::test]
async fn first_matching_pattern_wins() {
    let mw = StatusRetry::new()
        .for_pattern(r"/api/", vec![404])
        .for_pattern(r"/api/users", vec![]);
    let mut res = make_response("https://example.com/api/users", 404);
    assert!(mw.after_response(&mut res).await.is_err());
}

#[tokio::test]
async fn retry_after_seconds_are_exposed_as_retry_delay_hint() {
    let mw = StatusRetry::new();
    let mut res = make_response_with_retry_after("https://example.com/page", 429, "3");
    let err = mw.after_response(&mut res).await.unwrap_err();

    assert_eq!(
        mw.retry_delay(res.url(), &err),
        Some(Duration::from_secs(3))
    );
}

#[tokio::test]
async fn invalid_retry_after_header_does_not_set_retry_delay_hint() {
    let mw = StatusRetry::new();
    let mut res = make_response_with_retry_after("https://example.com/page", 429, "soon");
    let err = mw.after_response(&mut res).await.unwrap_err();

    assert_eq!(mw.retry_delay(res.url(), &err), None);
}

#[tokio::test]
async fn retry_after_http_date_is_exposed_as_retry_delay_hint() {
    let retry_at = SystemTime::now() + Duration::from_secs(2);
    let retry_after = httpdate::fmt_http_date(retry_at);
    let mw = StatusRetry::new();
    let mut res = make_response_with_retry_after("https://example.com/page", 503, &retry_after);
    let err = mw.after_response(&mut res).await.unwrap_err();
    let delay = mw.retry_delay(res.url(), &err).unwrap();

    assert!(delay <= Duration::from_secs(2));
    assert!(delay > Duration::ZERO);
}
