use crate::extract::{response::Response, selector::re_matches};

impl Response {
    /// Apply a regex pattern to the raw response body and return all matches.
    /// Returns an empty Vec if the body is binary.
    pub fn re(&self, pattern: &str) -> Vec<String> {
        match self.text() {
            Some(t) => re_matches(t, pattern),
            None => vec![],
        }
    }

    /// Return the first regex match in the response body, or `None`.
    pub fn re_first(&self, pattern: &str) -> Option<String> {
        self.re(pattern).into_iter().next()
    }
}
