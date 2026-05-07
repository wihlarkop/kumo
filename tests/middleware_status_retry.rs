use kumo::{
    error::KumoError,
    extract::Response,
    middleware::{Middleware, StatusRetry},
};

fn make_response(url: &str, status: u16) -> Response {
    Response::from_parts(url, status, "")
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
