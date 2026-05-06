mod core;
mod css;
mod json;
mod regex;
mod url;

#[cfg(feature = "xpath")]
mod xpath;

pub use core::Response;
pub(crate) use core::ResponseBody;

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn make_response(body: &str) -> Response {
        Response::from_parts("https://example.com", 200, body)
    }

    #[test]
    fn text_body_is_accessible() {
        let res = make_response("hello");
        assert_eq!(res.text(), Some("hello"));
        assert_eq!(res.bytes(), b"hello");
    }

    #[test]
    fn binary_body_text_returns_none() {
        let res = Response::from_bytes(
            "https://example.com",
            200,
            Bytes::from_static(b"\x89PNG\r\n"),
        );
        assert!(res.text().is_none());
        assert_eq!(&res.bytes()[..4], b"\x89PNG");
    }

    #[test]
    fn response_re_returns_matches() {
        let res = make_response("items: 5, total: 100");
        assert_eq!(res.re(r"\d+"), vec!["5", "100"]);
    }

    #[test]
    fn response_re_returns_capture_group_one() {
        let res = make_response("price: $42");
        assert_eq!(res.re(r"\$(\d+)"), vec!["42"]);
    }

    #[test]
    fn response_re_first_returns_first() {
        let res = make_response("1 and 2");
        assert_eq!(res.re_first(r"\d+"), Some("1".to_string()));
    }

    #[test]
    fn response_re_first_returns_none_when_no_match() {
        let res = make_response("no digits");
        assert_eq!(res.re_first(r"\d+"), None);
    }

    #[test]
    fn binary_body_re_returns_empty() {
        let res = Response::from_bytes("https://example.com", 200, Bytes::from_static(b"\xff\xfe"));
        assert!(res.re(r"\d+").is_empty());
    }

    #[cfg(feature = "jsonpath")]
    #[test]
    fn response_jsonpath_returns_values() {
        let res = make_response(r#"{"books":[{"title":"A"},{"title":"B"}]}"#);
        let vals = res.jsonpath("$.books[*].title").unwrap();
        let titles: Vec<&str> = vals.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(titles, vec!["A", "B"]);
    }

    #[cfg(feature = "jsonpath")]
    #[test]
    fn response_jsonpath_invalid_json_returns_error() {
        let res = make_response("not json");
        assert!(res.jsonpath("$.foo").is_err());
    }

    #[cfg(feature = "jsonpath")]
    #[test]
    fn response_jsonpath_invalid_path_returns_error() {
        let res = make_response(r#"{"a":1}"#);
        assert!(res.jsonpath("!!!bad").is_err());
    }

    #[cfg(feature = "xpath")]
    mod xpath_tests {
        use super::*;

        fn page(html: &str) -> Response {
            Response::from_parts("https://example.com", 200, html)
        }

        #[test]
        fn xpath_selects_element_text() {
            let res = page("<html><body><h1>Hello</h1></body></html>");
            let results = res.xpath("//h1");
            assert_eq!(results.len(), 1);
            assert!(results[0].contains("Hello"), "got: {:?}", results[0]);
        }

        #[test]
        fn xpath_extracts_attribute_value() {
            let res = page(r#"<html><body><a href="/next">Next</a></body></html>"#);
            let results = res.xpath("//a/@href");
            assert_eq!(results, vec!["/next"]);
        }

        #[test]
        fn xpath_extracts_text_node() {
            let res = page("<html><body><p>Hello world</p></body></html>");
            let results = res.xpath("//p/text()");
            assert_eq!(results, vec!["Hello world"]);
        }

        #[test]
        fn xpath_returns_multiple_matches() {
            let res = page("<html><body><ul><li>a</li><li>b</li><li>c</li></ul></body></html>");
            let results = res.xpath("//li/text()");
            assert_eq!(results, vec!["a", "b", "c"]);
        }

        #[test]
        fn xpath_returns_empty_on_no_match() {
            let res = page("<html><body><p>no span here</p></body></html>");
            assert!(res.xpath("//span").is_empty());
        }

        #[test]
        fn xpath_returns_empty_on_invalid_expr() {
            let res = page("<html><body></body></html>");
            assert!(res.xpath("!!!bad xpath").is_empty());
        }

        #[test]
        fn xpath_returns_empty_for_binary_body() {
            let res = Response::from_bytes(
                "https://example.com",
                200,
                bytes::Bytes::from_static(b"\xff\xfe"),
            );
            assert!(res.xpath("//p").is_empty());
        }

        #[test]
        fn xpath_first_returns_first_match() {
            let res = page("<html><body><p>one</p><p>two</p></body></html>");
            assert_eq!(res.xpath_first("//p/text()"), Some("one".to_string()));
        }

        #[test]
        fn xpath_first_returns_none_on_no_match() {
            let res = page("<html><body></body></html>");
            assert_eq!(res.xpath_first("//span"), None);
        }

        #[test]
        fn xpath_filtered_by_attribute() {
            let res = page(
                r#"<html><body>
                    <div class="price">$10</div>
                    <div class="title">Book</div>
                </body></html>"#,
            );
            let results = res.xpath(r#"//div[@class="price"]/text()"#);
            assert_eq!(results, vec!["$10"]);
        }
    }
}
