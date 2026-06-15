use std::time::Duration;

const DEFAULT_CONCURRENCY: usize = 8;
const DEFAULT_TCP_KEEPALIVE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub(crate) struct HttpClientPolicy {
    concurrency: usize,
    request_timeout: Option<Duration>,
    user_agent: String,
    tcp_keepalive: Duration,
}

impl HttpClientPolicy {
    pub(crate) fn new(
        concurrency: usize,
        request_timeout: Option<Duration>,
        user_agent: impl Into<String>,
    ) -> Self {
        Self {
            concurrency: concurrency.max(1),
            request_timeout,
            user_agent: user_agent.into(),
            tcp_keepalive: DEFAULT_TCP_KEEPALIVE,
        }
    }

    pub(crate) fn default_for(user_agent: impl Into<String>) -> Self {
        Self::new(DEFAULT_CONCURRENCY, None, user_agent)
    }

    #[cfg(test)]
    pub(crate) fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub(crate) fn reqwest_builder(&self) -> reqwest::ClientBuilder {
        let builder = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(&self.user_agent)
            .pool_max_idle_per_host(self.concurrency)
            .tcp_keepalive(self.tcp_keepalive);

        if let Some(timeout) = self.request_timeout {
            builder.timeout(timeout)
        } else {
            builder
        }
    }

    #[cfg(feature = "stealth")]
    pub(crate) fn wreq_builder(&self, emulation: wreq_util::Emulation) -> wreq::ClientBuilder {
        let builder = wreq::Client::builder()
            .emulation(emulation)
            .cookie_store(true)
            .pool_max_idle_per_host(self.concurrency)
            .tcp_keepalive(self.tcp_keepalive);

        if let Some(timeout) = self.request_timeout {
            builder.timeout(timeout)
        } else {
            builder
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HttpClientPolicy;
    use std::time::Duration;

    #[test]
    fn normalizes_zero_concurrency() {
        let policy = HttpClientPolicy::new(0, None, "test-agent");

        assert_eq!(policy.concurrency, 1);
    }

    #[test]
    fn retains_request_timeout() {
        let timeout = Duration::from_secs(15);
        let policy = HttpClientPolicy::new(4, Some(timeout), "test-agent");

        assert_eq!(policy.request_timeout, Some(timeout));
    }

    #[test]
    fn direct_fetcher_policy_uses_engine_defaults() {
        let policy = HttpClientPolicy::default_for("test-agent");

        assert_eq!(policy.concurrency, 8);
        assert_eq!(policy.request_timeout, None);
    }
}
