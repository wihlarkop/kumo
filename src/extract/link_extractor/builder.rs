use regex::Regex;

/// Collects, filters, and deduplicates hyperlinks from a [`Response`].
///
/// Eliminates the boilerplate `res.css("a").iter().filter_map(|el| el.attr("href"))...`
/// pattern that every spider writes manually.
///
/// [`Response`]: crate::extract::Response
///
/// # Example
/// ```rust,ignore
/// let links = LinkExtractor::new()
///     .allow_domains(&["example.com"])   // stay on-site
///     .allow(r"/product/\d+")            // only product pages
///     .deny(r"\.pdf$")                   // skip PDF links
///     .canonicalize(true)                // collapse /page#s1 and /page#s2 to /page
///     .extract(&response);
///
/// Output::new().follow_many(links)
/// ```
pub struct LinkExtractor {
    pub(super) allow: Vec<Regex>,
    pub(super) deny: Vec<Regex>,
    pub(super) restrict_css: Option<String>,
    pub(super) canonicalize: bool,
    pub(super) allow_domains: Vec<String>,
    pub(super) deny_domains: Vec<String>,
    pub(super) tags: Vec<String>,
    pub(super) attrs: Vec<String>,
}

impl LinkExtractor {
    pub fn new() -> Self {
        Self {
            allow: vec![],
            deny: vec![],
            restrict_css: None,
            canonicalize: false,
            allow_domains: vec![],
            deny_domains: vec![],
            tags: vec!["a".into(), "area".into()],
            attrs: vec!["href".into()],
        }
    }

    /// Only keep URLs matching this regex. Multiple calls are OR-ed together.
    /// Panics if `pattern` is not a valid regex.
    pub fn allow(mut self, pattern: &str) -> Self {
        self.allow.push(
            Regex::new(pattern)
                .unwrap_or_else(|e| panic!("invalid allow pattern '{pattern}': {e}")),
        );
        self
    }

    /// Drop URLs matching this regex. Multiple calls are OR-ed together.
    /// Panics if `pattern` is not a valid regex.
    pub fn deny(mut self, pattern: &str) -> Self {
        self.deny.push(
            Regex::new(pattern).unwrap_or_else(|e| panic!("invalid deny pattern '{pattern}': {e}")),
        );
        self
    }

    /// Limit link extraction to elements inside the first element matching `selector`.
    pub fn restrict_css(mut self, selector: &str) -> Self {
        self.restrict_css = Some(selector.to_string());
        self
    }

    /// Strip URL fragments (`#section`) before deduplication so that
    /// `/page#s1` and `/page#s2` collapse to a single `/page` entry.
    /// Default: `false`.
    pub fn canonicalize(mut self, enabled: bool) -> Self {
        self.canonicalize = enabled;
        self
    }

    /// Only keep URLs whose host is `domain` or any subdomain of it.
    /// Multiple calls are OR-ed together (same as `allow`).
    ///
    /// Example: `allow_domains(&["example.com"])` accepts `example.com`
    /// and `www.example.com` but not `notexample.com`.
    pub fn allow_domains(mut self, domains: &[&str]) -> Self {
        self.allow_domains
            .extend(domains.iter().map(|d| d.to_string()));
        self
    }

    /// Drop URLs whose host is `domain` or any subdomain of it.
    /// Multiple calls are OR-ed together (same as `deny`).
    pub fn deny_domains(mut self, domains: &[&str]) -> Self {
        self.deny_domains
            .extend(domains.iter().map(|d| d.to_string()));
        self
    }

    /// Set which HTML tags to extract links from.
    /// Default: `["a", "area"]`.
    pub fn tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|t| t.to_string()).collect();
        self
    }

    /// Set which HTML attributes are treated as link sources.
    /// Default: `["href"]`.
    pub fn attrs(mut self, attrs: &[&str]) -> Self {
        self.attrs = attrs.iter().map(|a| a.to_string()).collect();
        self
    }
}

impl Default for LinkExtractor {
    fn default() -> Self {
        Self::new()
    }
}
