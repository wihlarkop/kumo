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
            .map(|el| Element { outer_html: el.html() })
            .collect();
        ElementList { elements }
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
