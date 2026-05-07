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
    assert_eq!(output.follow, vec!["https://example.com/page/2"]);
}

#[test]
fn follow_many_adds_multiple_urls() {
    let urls = vec![
        "https://example.com/1".to_string(),
        "https://example.com/2".to_string(),
    ];
    let output = Output::<Item>::new().follow_many(urls.clone());
    assert_eq!(output.follow, urls);
}

#[test]
fn builder_is_chainable() {
    let output = Output::new()
        .item(Item { value: 99 })
        .follow("https://example.com/next");
    assert_eq!(output.follow, vec!["https://example.com/next"]);
}
