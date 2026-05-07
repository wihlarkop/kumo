use bytes::Bytes;
use kumo::extract::Response;

fn make_response(body: &str) -> Response {
    Response::from_parts("https://example.com", 200, body)
}

#[test]
fn text_body_is_accessible() {
    let res = make_response("<html>Hello</html>");
    assert_eq!(res.text(), Some("<html>Hello</html>"));
    assert_eq!(res.bytes(), b"<html>Hello</html>");
}

#[test]
fn binary_body_text_returns_none() {
    let res = Response::from_bytes("https://example.com", 200, Bytes::from_static(b"\xff\xfe"));
    assert_eq!(res.text(), None);
    assert_eq!(res.bytes(), b"\xff\xfe");
}

#[test]
fn response_re_returns_matches() {
    let res = make_response("item-123 item-456");
    assert_eq!(res.re(r"item-\d+"), vec!["item-123", "item-456"]);
}

#[test]
fn response_re_returns_capture_group_one() {
    let res = make_response("Price: $42");
    assert_eq!(res.re(r"\$(\d+)"), vec!["42"]);
}

#[test]
fn response_re_first_returns_first() {
    let res = make_response("1 2 3");
    assert_eq!(res.re_first(r"\d+"), Some("1".to_string()));
}

#[test]
fn response_re_first_returns_none_when_no_match() {
    let res = make_response("abc");
    assert_eq!(res.re_first(r"\d+"), None);
}

#[test]
fn binary_body_re_returns_empty() {
    let res = Response::from_bytes("https://example.com", 200, Bytes::from_static(b"\xff\xfe"));
    assert!(res.re(r"\d+").is_empty());
}

#[test]
fn response_json_deserializes_from_text_body() {
    let res = make_response(r#"{"name":"kumo","count":2}"#);
    let value: serde_json::Value = res.json().unwrap();
    assert_eq!(value["name"], "kumo");
    assert_eq!(value["count"], 2);
}

#[test]
fn response_json_deserializes_from_binary_body() {
    let res = Response::from_bytes(
        "https://example.com",
        200,
        Bytes::from_static(br#"{"ok":true}"#),
    );
    let value: serde_json::Value = res.json().unwrap();
    assert_eq!(value["ok"], true);
}

#[test]
#[cfg(feature = "jsonpath")]
fn response_jsonpath_selects_values() {
    let res = make_response(r#"{"items":[{"name":"a"},{"name":"b"}]}"#);
    let values = res.jsonpath("$.items[*].name").unwrap();
    assert_eq!(values, vec![serde_json::json!("a"), serde_json::json!("b")]);
}

#[test]
#[cfg(feature = "xpath")]
fn response_xpath_selects_text() {
    let res = make_response("<html><body><h1>Hello</h1><h1>World</h1></body></html>");
    assert_eq!(res.xpath("//h1/text()"), vec!["Hello", "World"]);
}

#[test]
#[cfg(feature = "xpath")]
fn response_xpath_first_returns_first() {
    let res = make_response("<html><body><p>A</p><p>B</p></body></html>");
    assert_eq!(res.xpath_first("//p/text()"), Some("A".to_string()));
}
