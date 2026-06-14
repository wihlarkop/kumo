use std::{collections::HashMap, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainPolicy {
    per_domain_concurrency: usize,
    per_domain_delay: Option<Duration>,
}

impl DomainPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn per_domain_concurrency(mut self, value: usize) -> Self {
        self.per_domain_concurrency = value.max(1);
        self
    }

    pub fn per_domain_delay(mut self, delay: Duration) -> Self {
        self.per_domain_delay = Some(delay);
        self
    }

    pub fn concurrency(&self) -> usize {
        self.per_domain_concurrency
    }

    pub fn delay(&self) -> Option<Duration> {
        self.per_domain_delay
    }
}

impl Default for DomainPolicy {
    fn default() -> Self {
        Self {
            per_domain_concurrency: 8,
            per_domain_delay: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolitenessPolicy {
    default: DomainPolicy,
    jitter: Option<Duration>,
    respect_robots_crawl_delay: bool,
    domains: HashMap<String, DomainPolicy>,
}

impl PolitenessPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn per_domain_concurrency(mut self, value: usize) -> Self {
        self.default = self.default.per_domain_concurrency(value);
        self
    }

    pub fn per_domain_delay(mut self, delay: Duration) -> Self {
        self.default = self.default.per_domain_delay(delay);
        self
    }

    pub fn jitter(mut self, jitter: Duration) -> Self {
        self.jitter = Some(jitter);
        self
    }

    pub fn respect_robots_crawl_delay(mut self, value: bool) -> Self {
        self.respect_robots_crawl_delay = value;
        self
    }

    pub fn domain(mut self, domain: impl Into<String>, policy: DomainPolicy) -> Self {
        self.domains
            .insert(domain.into().to_ascii_lowercase(), policy);
        self
    }

    pub fn policy_for(&self, domain: &str) -> &DomainPolicy {
        self.domains
            .get(&domain.to_ascii_lowercase())
            .unwrap_or(&self.default)
    }

    pub fn default_per_domain_concurrency(&self) -> usize {
        self.default.concurrency()
    }

    pub fn default_per_domain_delay(&self) -> Option<Duration> {
        self.default.delay()
    }

    pub fn jitter_range(&self) -> Option<Duration> {
        self.jitter
    }

    pub fn respects_robots_crawl_delay(&self) -> bool {
        self.respect_robots_crawl_delay
    }
}

impl Default for PolitenessPolicy {
    fn default() -> Self {
        Self {
            default: DomainPolicy::default(),
            jitter: None,
            respect_robots_crawl_delay: true,
            domains: HashMap::new(),
        }
    }
}
