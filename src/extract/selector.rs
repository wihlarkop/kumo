mod cache;
mod element;
mod list;
mod regex;

pub(crate) use cache::get_selector;
pub use element::Element;
pub use list::ElementList;
pub(crate) use regex::re_matches;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn selector_cache_reuses_compiled_selector() {
        let s1 = get_selector("h1");
        let s2 = get_selector("h1");
        assert!(s1.is_some());
        assert!(s2.is_some());
        assert!(Arc::ptr_eq(s1.as_ref().unwrap(), s2.as_ref().unwrap()));
    }

    #[test]
    fn selector_cache_returns_none_for_invalid() {
        assert!(get_selector("!!!bad").is_none());
    }

    fn make_element(html: &str) -> Element {
        Element {
            outer_html: html.to_string(),
        }
    }

    #[test]
    fn element_text_returns_concatenated_text() {
        let el = make_element("<p>Hello <strong>world</strong></p>");
        assert_eq!(el.text(), "Hello world");
    }

    #[test]
    fn element_text_strips_tags() {
        let el = make_element("<span>  kumo  </span>");
        assert_eq!(el.text(), "  kumo  ");
    }

    #[test]
    fn element_attr_returns_value() {
        let el = make_element(r#"<a href="/next">Next</a>"#);
        assert_eq!(el.attr("href"), Some("/next".to_string()));
    }

    #[test]
    fn element_attr_returns_none_for_missing() {
        let el = make_element("<a>No href</a>");
        assert_eq!(el.attr("href"), None);
    }

    #[test]
    fn element_inner_html_excludes_outer_tag() {
        let el = make_element("<div><span>inner</span></div>");
        assert_eq!(el.inner_html(), "<span>inner</span>");
    }

    #[test]
    fn element_css_selects_children() {
        let el = make_element("<ul><li>a</li><li>b</li></ul>");
        let items = el.css("li");
        assert_eq!(items.len(), 2);
        assert_eq!(items.first().unwrap().text(), "a");
    }

    #[test]
    fn element_css_bad_selector_returns_empty() {
        let el = make_element("<div>x</div>");
        let result = el.css("!!!bad");
        assert!(result.is_empty());
    }

    #[test]
    fn element_list_iter_yields_all_elements() {
        let list = ElementList {
            elements: vec![
                make_element("<span>a</span>"),
                make_element("<span>b</span>"),
            ],
        };
        let texts: Vec<String> = list.iter().map(|e| e.text()).collect();
        assert_eq!(texts, vec!["a", "b"]);
    }

    #[test]
    fn element_list_first_returns_first() {
        let list = ElementList {
            elements: vec![
                make_element("<span>first</span>"),
                make_element("<span>second</span>"),
            ],
        };
        assert_eq!(list.first().unwrap().text(), "first");
    }

    #[test]
    fn element_list_is_empty_when_empty() {
        let list = ElementList { elements: vec![] };
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn element_re_returns_full_match_without_group() {
        let el = make_element("<p>Price: $42</p>");
        assert_eq!(el.re(r"\$\d+"), vec!["$42"]);
    }

    #[test]
    fn element_re_returns_capture_group_one() {
        let el = make_element("<p>Price: $42</p>");
        assert_eq!(el.re(r"\$(\d+)"), vec!["42"]);
    }

    #[test]
    fn element_re_first_returns_first_match() {
        let el = make_element("<p>1 and 2 and 3</p>");
        assert_eq!(el.re_first(r"\d+"), Some("1".to_string()));
    }

    #[test]
    fn element_re_first_returns_none_when_no_match() {
        let el = make_element("<p>no numbers here</p>");
        assert_eq!(el.re_first(r"\d+"), None);
    }

    #[test]
    fn element_re_invalid_pattern_returns_empty() {
        let el = make_element("<p>text</p>");
        assert!(el.re("(unclosed").is_empty());
    }

    #[test]
    fn element_list_re_flattens_across_elements() {
        let list = ElementList {
            elements: vec![
                make_element("<span>$10</span>"),
                make_element("<span>$20</span>"),
            ],
        };
        assert_eq!(list.re(r"\$(\d+)"), vec!["10", "20"]);
    }

    #[test]
    fn element_list_re_first_returns_first_across_elements() {
        let list = ElementList {
            elements: vec![
                make_element("<span>$10</span>"),
                make_element("<span>$20</span>"),
            ],
        };
        assert_eq!(list.re_first(r"\$(\d+)"), Some("10".to_string()));
    }
}
