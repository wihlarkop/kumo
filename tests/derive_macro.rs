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

#[derive(Extract, Debug)]
struct ProductScalars {
    #[extract(css = ".name", transform = "trim")]
    name: String,
    #[extract(css = ".price", re = r"[\d.]+")]
    price: f64,
    #[extract(css = ".stock", re = r"\d+")]
    stock: u32,
    #[extract(css = ".featured")]
    featured: bool,
    #[extract(css = ".missing-count")]
    missing_count: Option<u16>,
}

#[derive(Extract, Debug)]
struct ProductLists {
    #[extract(css = ".tag", transform = "lowercase")]
    tags: Vec<String>,
    #[extract(css = ".score", re = r"\d+")]
    scores: Vec<u8>,
}

#[derive(Extract, Debug)]
#[allow(dead_code)]
struct InvalidScalar {
    #[extract(css = ".stock")]
    stock: u32,
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

#[tokio::test]
async fn derive_parses_scalars_and_optional_scalars() {
    let response = Response::from_parts(
        "https://example.com",
        200,
        r#"
        <article>
            <h1 class="name"> Widget </h1>
            <span class="price">$12.50</span>
            <span class="stock">7 in stock</span>
            <span class="featured">true</span>
        </article>
        "#,
    );
    let elements = response.css("article");
    let element = elements.first().unwrap();

    let extracted = ProductScalars::extract_from(element, None).await.unwrap();

    assert_eq!(extracted.name, "Widget");
    assert_eq!(extracted.price, 12.50);
    assert_eq!(extracted.stock, 7);
    assert!(extracted.featured);
    assert_eq!(extracted.missing_count, None);
}

#[tokio::test]
async fn derive_extracts_vec_fields_from_all_matching_elements() {
    let response = Response::from_parts(
        "https://example.com",
        200,
        r#"
        <article>
            <span class="tag">Rust</span>
            <span class="tag">Scraping</span>
            <span class="score">10 points</span>
            <span class="score">42 points</span>
        </article>
        "#,
    );
    let elements = response.css("article");
    let element = elements.first().unwrap();

    let extracted = ProductLists::extract_from(element, None).await.unwrap();

    assert_eq!(extracted.tags, ["rust", "scraping"]);
    assert_eq!(extracted.scores, [10, 42]);
}

#[tokio::test]
async fn derive_scalar_parse_errors_include_field_name() {
    let response = Response::from_parts(
        "https://example.com",
        200,
        r#"<article><span class="stock">many</span></article>"#,
    );
    let elements = response.css("article");
    let element = elements.first().unwrap();

    let error = InvalidScalar::extract_from(element, None)
        .await
        .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("stock"), "{message}");
    assert!(message.contains("u32"), "{message}");
    assert!(message.contains("many"), "{message}");
}
