use crate::extract::response::Response;

impl Response {
    /// Resolve a relative URL against this response's URL.
    pub fn urljoin(&self, path: &str) -> String {
        use url::Url;
        Url::parse(&self.url)
            .and_then(|base| base.join(path))
            .map(|u| u.to_string())
            .unwrap_or_else(|_| path.to_string())
    }
}
