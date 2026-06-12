use crate::extract::{
    response::Response,
    selector::{CssSelector, Element, ElementList},
};

impl Response {
    /// Select elements in this page via a CSS selector.
    /// Returns an empty list if the body is binary.
    pub fn css(&self, selector: &str) -> ElementList {
        let Some(sel) = crate::extract::selector::get_selector(selector) else {
            return ElementList { elements: vec![] };
        };
        self.css_with(&sel)
    }

    /// Select elements using a reusable, precompiled CSS selector.
    pub fn css_with(&self, selector: &CssSelector) -> ElementList {
        let Some(document) = self.document() else {
            return ElementList { elements: vec![] };
        };
        let node_ids = document
            .lock()
            .expect("parsed document lock should not be poisoned")
            .select(selector.as_scraper())
            .map(|element| element.id())
            .collect::<Vec<_>>();
        let elements = node_ids
            .into_iter()
            .map(|node_id| Element::new(document.clone(), node_id))
            .collect();
        ElementList { elements }
    }
}
