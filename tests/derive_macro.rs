//! Compile-time tests for the #[derive(Extract)] macro.
//!
//! Pass cases verify correct usage compiles cleanly.
//! Fail cases verify that misuse produces clear, helpful error messages.

use kumo::prelude::*;

#[derive(Extract)]
struct LinkWithId {
    #[extract(css = "a", attr = "href", re = r"/products/(\d+)")]
    id: String,
}

#[test]
fn derive_pass_cases() {
    let t = trybuild::TestCases::new();
    t.pass("tests/derive/pass/*.rs");
}

#[test]
fn derive_fail_cases() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/derive/fail/*.rs");
}

#[tokio::test]
async fn attr_regex_applies_regex_to_attribute_value() {
    let response = Response::from_parts(
        "https://example.com",
        200,
        r#"<div><a href="/products/123">Widget</a></div>"#,
    );
    let elements = response.css("div");
    let element = elements.first().unwrap();

    let extracted = LinkWithId::extract_from(element, None).await.unwrap();

    assert_eq!(extracted.id, "123");
}
