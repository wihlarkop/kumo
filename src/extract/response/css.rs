use crate::extract::{
    response::Response,
    selector::{Element, ElementList},
};

impl Response {
    /// Select elements in this page via a CSS selector.
    /// Returns an empty list if the body is binary.
    pub fn css(&self, selector: &str) -> ElementList {
        let Some(document) = self.document() else {
            return ElementList { elements: vec![] };
        };
        let Some(sel) = crate::extract::selector::get_selector(selector) else {
            return ElementList { elements: vec![] };
        };
        let node_ids = document
            .lock()
            .expect("parsed document lock should not be poisoned")
            .select(&sel)
            .map(|element| element.id())
            .collect::<Vec<_>>();
        let elements = node_ids
            .into_iter()
            .map(|node_id| Element::new(document.clone(), node_id))
            .collect();
        ElementList { elements }
    }
}
