use super::Element;

/// A list of HTML elements matched by a CSS selector.
#[derive(Clone, Debug)]
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
