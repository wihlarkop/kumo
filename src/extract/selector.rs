pub(crate) fn re_matches(text: &str, pattern: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(pattern) else {
        return vec![];
    };
    re.captures_iter(text)
        .map(|cap| {
            cap.get(1)
                .unwrap_or_else(|| cap.get(0).unwrap())
                .as_str()
                .to_string()
        })
        .collect()
}

/// A list of HTML elements matched by a CSS selector.
pub struct ElementList {
    pub(crate) elements: Vec<Element>,
}

impl ElementList {
    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        self.elements.iter()
    }

    pub fn first(&self) -> Option<&Element> {
        self.elements.first()
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Apply a regex pattern to the text of each element and return all matches.
    ///
    /// If the pattern contains capture group 1, returns group-1 matches.
    /// Otherwise returns the full match. Returns an empty Vec on invalid pattern.
    pub fn re(&self, pattern: &str) -> Vec<String> {
        self.elements.iter().flat_map(|el| el.re(pattern)).collect()
    }

    /// Return the first regex match across all elements, or `None`.
    pub fn re_first(&self, pattern: &str) -> Option<String> {
        self.elements.iter().find_map(|el| el.re_first(pattern))
    }
}

/// A single CSS-matched HTML element.
///
/// Stores the element's outer HTML so it can be queried independently
/// of the parent document lifetime.
pub struct Element {
    pub(crate) outer_html: String,
}

impl Element {
    /// Get the concatenated text content of this element and all its descendants.
    pub fn text(&self) -> String {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        fragment.root_element().text().collect::<Vec<_>>().join("")
    }

    /// Get the value of an attribute by name.
    pub fn attr(&self, name: &str) -> Option<String> {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        let sel = scraper::Selector::parse("*").unwrap();
        // parse_fragment wraps content in html>body; skip those synthetic nodes.
        fragment
            .select(&sel)
            .find(|el| !matches!(el.value().name(), "html" | "body"))
            .and_then(|el| el.value().attr(name))
            .map(String::from)
    }

    /// Select child elements via a CSS selector.
    pub fn css(&self, selector: &str) -> ElementList {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        let Ok(sel) = scraper::Selector::parse(selector) else {
            return ElementList { elements: vec![] };
        };
        let elements = fragment
            .select(&sel)
            .map(|el| Element {
                outer_html: el.html(),
            })
            .collect();
        ElementList { elements }
    }

    /// Apply a regex pattern to this element's text content and return all matches.
    ///
    /// If the pattern contains capture group 1, returns group-1 matches.
    /// Otherwise returns the full match. Returns an empty Vec on invalid pattern.
    pub fn re(&self, pattern: &str) -> Vec<String> {
        re_matches(&self.text(), pattern)
    }

    /// Return the first regex match in this element's text, or `None`.
    pub fn re_first(&self, pattern: &str) -> Option<String> {
        self.re(pattern).into_iter().next()
    }

    /// Get the inner HTML of this element (children only, no outer tag).
    pub fn inner_html(&self) -> String {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        let sel = scraper::Selector::parse("*").unwrap();
        fragment
            .select(&sel)
            .find(|el| !matches!(el.value().name(), "html" | "body"))
            .map(|el| el.inner_html())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
