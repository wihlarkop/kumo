use kumo::extract::{CssSelector, Response};

fn first_element(html: &str, selector: &str) -> kumo::extract::Element {
    Response::from_parts("https://example.com", 200, html)
        .css(selector)
        .first()
        .cloned()
        .unwrap()
}

#[test]
fn element_text_returns_concatenated_text() {
    let el = first_element("<p>Hello <strong>world</strong></p>", "p");
    assert_eq!(el.text(), "Hello world");
}

#[test]
fn element_text_strips_tags() {
    let el = first_element("<span>  kumo  </span>", "span");
    assert_eq!(el.text(), "  kumo  ");
}

#[test]
fn element_attr_returns_value() {
    let el = first_element(r#"<a href="/next">Next</a>"#, "a");
    assert_eq!(el.attr("href"), Some("/next".to_string()));
}

#[test]
fn element_attr_returns_none_for_missing() {
    let el = first_element("<a>No href</a>", "a");
    assert_eq!(el.attr("href"), None);
}

#[test]
fn element_inner_html_excludes_outer_tag() {
    let el = first_element("<div><span>inner</span></div>", "div");
    assert_eq!(el.inner_html(), "<span>inner</span>");
}

#[test]
fn element_outer_html_includes_element_and_is_stable() {
    let el = first_element(r#"<a href="/next"><strong>Next</strong></a>"#, "a");

    assert_eq!(
        el.outer_html(),
        r#"<a href="/next"><strong>Next</strong></a>"#
    );
    assert_eq!(
        el.outer_html(),
        r#"<a href="/next"><strong>Next</strong></a>"#
    );
}

#[test]
fn element_css_selects_children() {
    let el = first_element("<ul><li>a</li><li>b</li></ul>", "ul");
    let items = el.css("li");
    assert_eq!(items.len(), 2);
    assert_eq!(items.first().unwrap().text(), "a");
}

#[test]
fn cloned_element_remains_queryable_after_response_is_dropped() {
    let element = {
        let response = Response::from_parts(
            "https://example.com",
            200,
            r#"<article><a href="/book">Book</a></article>"#,
        );
        response.css("article").first().cloned().unwrap()
    };

    let link = element.css(":scope > a").first().cloned().unwrap();
    assert_eq!(link.text(), "Book");
    assert_eq!(link.attr("href"), Some("/book".to_string()));
}

#[test]
fn element_css_bad_selector_returns_empty() {
    let el = first_element("<div>x</div>", "div");
    let result = el.css("!!!bad");
    assert!(result.is_empty());
}

#[test]
fn element_list_iter_yields_all_elements() {
    let response = Response::from_parts("https://example.com", 200, "<span>a</span><span>b</span>");
    let list = response.css("span");
    let texts: Vec<String> = list.iter().map(|e| e.text()).collect();
    assert_eq!(texts, vec!["a", "b"]);
}

#[test]
fn element_list_first_returns_first() {
    let response = Response::from_parts(
        "https://example.com",
        200,
        "<span>first</span><span>second</span>",
    );
    let list = response.css("span");
    assert_eq!(list.first().unwrap().text(), "first");
}

#[test]
fn element_list_is_empty_when_empty() {
    let response = Response::from_parts("https://example.com", 200, "<div>x</div>");
    let list = response.css("span");
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
}

#[test]
fn element_re_returns_full_match_without_group() {
    let el = first_element("<p>Price: $42</p>", "p");
    assert_eq!(el.re(r"\$\d+"), vec!["$42"]);
}

#[test]
fn element_re_returns_capture_group_one() {
    let el = first_element("<p>Price: $42</p>", "p");
    assert_eq!(el.re(r"\$(\d+)"), vec!["42"]);
}

#[test]
fn element_re_first_returns_first_match() {
    let el = first_element("<p>1 and 2 and 3</p>", "p");
    assert_eq!(el.re_first(r"\d+"), Some("1".to_string()));
}

#[test]
fn element_re_first_returns_none_when_no_match() {
    let el = first_element("<p>no numbers here</p>", "p");
    assert_eq!(el.re_first(r"\d+"), None);
}

#[test]
fn element_re_invalid_pattern_returns_empty() {
    let el = first_element("<p>text</p>", "p");
    assert!(el.re("(unclosed").is_empty());
}

#[test]
fn element_list_re_flattens_across_elements() {
    let response = Response::from_parts(
        "https://example.com",
        200,
        "<span>$10</span><span>$20</span>",
    );
    let list = response.css("span");
    assert_eq!(list.re(r"\$(\d+)"), vec!["10", "20"]);
}

#[test]
fn element_list_re_first_returns_first_across_elements() {
    let response = Response::from_parts(
        "https://example.com",
        200,
        "<span>$10</span><span>$20</span>",
    );
    let list = response.css("span");
    assert_eq!(list.re_first(r"\$(\d+)"), Some("10".to_string()));
}

#[test]
fn compiled_selector_can_be_reused_across_responses() {
    let selector = CssSelector::parse("article > h2").unwrap();
    let first = Response::from_parts(
        "https://example.com/first",
        200,
        "<article><h2>First</h2></article>",
    );
    let second = Response::from_parts(
        "https://example.com/second",
        200,
        "<article><h2>Second</h2></article>",
    );

    assert_eq!(first.css_with(&selector).first().unwrap().text(), "First");
    assert_eq!(second.css_with(&selector).first().unwrap().text(), "Second");
}

#[test]
fn compiled_selector_supports_nested_queries() {
    let article = first_element(
        "<article><a href=\"/one\">One</a><a href=\"/two\">Two</a></article>",
        "article",
    );
    let links = CssSelector::parse(":scope > a").unwrap();

    let matches = article.css_with(&links);

    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches.first().unwrap().attr("href").as_deref(),
        Some("/one")
    );
}

#[test]
fn compiled_selector_rejects_invalid_css() {
    let error = CssSelector::parse("!!!bad").unwrap_err();

    assert_eq!(error.kind_label(), "parse");
    assert!(error.to_string().contains("CSS selector"));
}

#[test]
fn compiled_selector_is_cloneable_and_thread_safe() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CssSelector>();

    let selector = CssSelector::parse("p").unwrap();
    let cloned = selector.clone();
    let selected = std::thread::spawn(move || {
        Response::from_parts("https://example.com", 200, "<p>threaded</p>")
            .css_with(&cloned)
            .first()
            .unwrap()
            .text()
    })
    .join()
    .unwrap();

    assert_eq!(selected, "threaded");
}
