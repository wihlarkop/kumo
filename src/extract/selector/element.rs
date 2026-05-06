use super::{ElementList, get_selector, re_matches};

static ROOT_SELECTOR: std::sync::LazyLock<scraper::Selector> =
    std::sync::LazyLock::new(|| scraper::Selector::parse("*").unwrap());

/// A single CSS-matched HTML element.
///
/// Stores the element's outer HTML so it can be queried independently
/// of the parent document lifetime.
#[derive(Clone, Debug)]
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
        // parse_fragment wraps content in html>body; skip those synthetic nodes.
        fragment
            .select(&ROOT_SELECTOR)
            .find(|el| !matches!(el.value().name(), "html" | "body"))
            .and_then(|el| el.value().attr(name))
            .map(String::from)
    }

    /// Select child elements via a CSS selector.
    pub fn css(&self, selector: &str) -> ElementList {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        let Some(sel) = get_selector(selector) else {
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

    /// Get the outer HTML of this element (the element itself and its children).
    pub fn outer_html(&self) -> &str {
        &self.outer_html
    }

    /// Get the inner HTML of this element (children only, no outer tag).
    pub fn inner_html(&self) -> String {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        fragment
            .select(&ROOT_SELECTOR)
            .find(|el| !matches!(el.value().name(), "html" | "body"))
            .map(|el| el.inner_html())
            .unwrap_or_default()
    }
}
