use kumo::CrawlRequest;
use kumo::spider::Output;
use serde::Serialize;

#[derive(Serialize)]
struct Item {
    value: i32,
}

#[test]
fn new_output_has_no_follow_urls() {
    let output = Output::<Item>::new();
    assert!(output.follow.is_empty());
}

#[test]
fn default_output_has_no_follow_urls() {
    let output = Output::<Item>::default();
    assert!(output.follow.is_empty());
}

#[test]
fn follow_adds_url() {
    let output = Output::<Item>::new().follow("https://example.com/page/2");
    assert_eq!(output.follow[0].url(), "https://example.com/page/2");
    assert_eq!(output.follow[0].priority_value(), 0);
}

#[test]
fn follow_many_adds_multiple_urls() {
    let urls = vec![
        "https://example.com/1".to_string(),
        "https://example.com/2".to_string(),
    ];
    let output = Output::<Item>::new().follow_many(urls.clone());
    let followed: Vec<_> = output.follow.iter().map(|r| r.url()).collect();
    assert_eq!(followed, vec![urls[0].as_str(), urls[1].as_str()]);
}

#[test]
fn request_adds_crawl_request() {
    let output = Output::<Item>::new().request(
        CrawlRequest::get("https://example.com/high")
            .priority(10)
            .meta("kind", "listing")
            .dont_filter(true),
    );
    let request = &output.follow[0];
    assert_eq!(request.url(), "https://example.com/high");
    assert_eq!(request.priority_value(), 10);
    assert_eq!(
        request.meta_value("kind"),
        Some(&serde_json::json!("listing"))
    );
    assert!(request.dont_filter_enabled());
}

#[test]
fn requests_add_multiple_crawl_requests() {
    let output = Output::<Item>::new().requests(vec![
        CrawlRequest::get("https://example.com/a"),
        CrawlRequest::get("https://example.com/b").priority(5),
    ]);
    let followed: Vec<_> = output.follow.iter().map(|r| r.url()).collect();
    assert_eq!(
        followed,
        vec!["https://example.com/a", "https://example.com/b"]
    );
    assert_eq!(output.follow[1].priority_value(), 5);
}

#[test]
fn builder_is_chainable() {
    let output = Output::new()
        .item(Item { value: 99 })
        .follow("https://example.com/next");
    assert_eq!(output.follow[0].url(), "https://example.com/next");
}
