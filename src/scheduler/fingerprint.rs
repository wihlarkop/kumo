#[derive(Debug, Clone)]
pub struct FingerprintPolicy {
    strip_tracking_params: bool,
    sort_query: bool,
}

impl FingerprintPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn strip_tracking_params(mut self, value: bool) -> Self {
        self.strip_tracking_params = value;
        self
    }

    pub fn sort_query(mut self, value: bool) -> Self {
        self.sort_query = value;
        self
    }

    pub fn fingerprint(&self, raw_url: &str) -> Result<String, url::ParseError> {
        let mut url = url::Url::parse(raw_url)?;
        if let Some(host) = url.host_str().map(str::to_ascii_lowercase) {
            url.set_host(Some(&host))?;
        }
        url.set_fragment(None);

        let mut pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(key, _)| {
                !self.strip_tracking_params
                    || !(key.starts_with("utm_") || key == "fbclid" || key == "gclid")
            })
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();

        if self.sort_query {
            pairs.sort();
        }

        url.set_query(None);
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }

        Ok(url.to_string())
    }
}

impl Default for FingerprintPolicy {
    fn default() -> Self {
        Self {
            strip_tracking_params: false,
            sort_query: true,
        }
    }
}
