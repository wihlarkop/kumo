use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use rand::Rng;
use tokio::sync::Mutex;

use crate::{
    frontier::Frontier,
    request::{CrawlRequest, FrontierRequest},
};

use super::{fingerprint::FingerprintPolicy, policy::PolitenessPolicy};

#[derive(Debug, Default)]
struct DomainState {
    in_flight: usize,
    next_available_at: Option<Instant>,
    robots_delay: Option<Duration>,
}

pub struct CrawlScheduler {
    frontier: Arc<dyn Frontier>,
    policy: PolitenessPolicy,
    fingerprint_policy: FingerprintPolicy,
    domains: Mutex<HashMap<String, DomainState>>,
}

pub(crate) enum SchedulerPoll {
    Ready(Box<FrontierRequest>),
    Pending(Duration),
    Empty,
}

enum CandidateState {
    Ready,
    Pending(Duration),
}

fn delay_with_jitter(base: Duration, jitter: Option<Duration>) -> Duration {
    let Some(jitter) = jitter else {
        return base;
    };
    if jitter.is_zero() {
        return base;
    }

    let extra = rand::rng().random_range(Duration::ZERO..=jitter);
    base.saturating_add(extra)
}

fn domain_state_mut<'a>(
    domains: &'a mut HashMap<String, DomainState>,
    domain: &str,
) -> &'a mut DomainState {
    if !domains.contains_key(domain) {
        domains.insert(domain.to_string(), DomainState::default());
    }
    domains
        .get_mut(domain)
        .expect("domain state was inserted before lookup")
}

impl CrawlScheduler {
    pub fn new(frontier: impl Frontier + 'static, policy: PolitenessPolicy) -> Self {
        Self::from_arc(Arc::new(frontier), policy)
    }

    pub fn from_arc(frontier: Arc<dyn Frontier>, policy: PolitenessPolicy) -> Self {
        Self {
            frontier,
            policy,
            fingerprint_policy: FingerprintPolicy::default(),
            domains: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_fingerprint_policy(mut self, policy: FingerprintPolicy) -> Self {
        self.fingerprint_policy = policy;
        self
    }

    pub async fn push_request(&self, request: CrawlRequest, depth: usize) -> bool {
        self.frontier
            .push_request(self.apply_fingerprint(request), depth)
            .await
    }

    pub async fn push_request_force(&self, queued: FrontierRequest) {
        self.frontier.push_request_force(queued).await;
    }

    pub async fn is_empty(&self) -> bool {
        self.frontier.is_empty().await
    }

    pub async fn flush(&self) -> Result<(), crate::error::KumoError> {
        self.frontier.flush().await
    }

    pub async fn try_next_ready(&self) -> Option<FrontierRequest> {
        match self.poll_next().await {
            SchedulerPoll::Ready(queued) => Some(*queued),
            SchedulerPoll::Pending(_) | SchedulerPoll::Empty => None,
        }
    }

    pub(crate) async fn poll_ready(&self) -> SchedulerPoll {
        self.poll_next().await
    }

    pub async fn next_ready(&self) -> Option<FrontierRequest> {
        loop {
            match self.poll_next().await {
                SchedulerPoll::Ready(queued) => return Some(*queued),
                SchedulerPoll::Pending(wait) => tokio::time::sleep(wait).await,
                SchedulerPoll::Empty => return None,
            }
        }
    }

    pub async fn finish(&self, queued: &FrontierRequest) {
        let Some(domain) = queued.request().domain_key() else {
            return;
        };

        let mut domains = self.domains.lock().await;
        let state = domain_state_mut(&mut domains, domain);
        state.in_flight = state.in_flight.saturating_sub(1);
        let policy_delay = self.policy.policy_for(domain).delay();
        let robots_delay = if self.policy.respects_robots_crawl_delay() {
            state.robots_delay
        } else {
            None
        };
        if let Some(delay) = [policy_delay, robots_delay].into_iter().flatten().max() {
            let delay = delay_with_jitter(delay, self.policy.jitter_range());
            state.next_available_at = Some(Instant::now() + delay);
        }
    }

    pub async fn observe_robots_crawl_delay(&self, url: &str, delay: Duration) {
        let request = CrawlRequest::get(url);
        self.observe_request_robots_crawl_delay(&request, delay)
            .await;
    }

    pub(crate) async fn observe_request_robots_crawl_delay(
        &self,
        request: &CrawlRequest,
        delay: Duration,
    ) {
        let Some(domain) = request.domain_key() else {
            return;
        };

        let mut domains = self.domains.lock().await;
        let state = domain_state_mut(&mut domains, domain);
        state.robots_delay = Some(delay);
    }

    async fn poll_next(&self) -> SchedulerPoll {
        let queued_len = self.frontier.len().await;
        if queued_len == 0 {
            return SchedulerPoll::Empty;
        };

        let candidates = self.frontier.pop_request_batch(queued_len).await;
        if candidates.is_empty() {
            return SchedulerPoll::Empty;
        }

        let mut deferred = Vec::with_capacity(candidates.len());
        let mut shortest_wait: Option<Duration> = None;
        let mut candidates = candidates.into_iter();

        while let Some(queued) = candidates.next() {
            match self.classify_candidate(&queued).await {
                CandidateState::Ready => {
                    deferred.extend(candidates);
                    self.frontier.restore_request_batch(deferred).await;
                    return SchedulerPoll::Ready(Box::new(queued));
                }
                CandidateState::Pending(wait) => {
                    shortest_wait = Some(shortest_wait.map_or(wait, |current| current.min(wait)));
                    deferred.push(queued);
                }
            }
        }

        self.frontier.restore_request_batch(deferred).await;

        shortest_wait.map_or(SchedulerPoll::Empty, SchedulerPoll::Pending)
    }

    async fn classify_candidate(&self, queued: &FrontierRequest) -> CandidateState {
        if let Some(scheduled_at) = queued.scheduled_at()
            && let Ok(wait) = scheduled_at.duration_since(std::time::SystemTime::now())
        {
            return CandidateState::Pending(wait);
        }

        let Some(domain) = queued.request().domain_key() else {
            return CandidateState::Ready;
        };

        let mut domains = self.domains.lock().await;
        let state = domain_state_mut(&mut domains, domain);
        let domain_policy = self.policy.policy_for(domain);

        if state.in_flight >= domain_policy.concurrency() {
            CandidateState::Pending(Duration::from_millis(10))
        } else if let Some(next) = state.next_available_at {
            match next.checked_duration_since(Instant::now()) {
                Some(wait) => CandidateState::Pending(wait),
                None => {
                    state.in_flight += 1;
                    CandidateState::Ready
                }
            }
        } else {
            state.in_flight += 1;
            CandidateState::Ready
        }
    }

    fn apply_fingerprint(&self, request: CrawlRequest) -> CrawlRequest {
        if request.dont_filter_enabled() {
            return request;
        }

        let key = request
            .parsed_url()
            .and_then(|url| self.fingerprint_policy.fingerprint_parsed(url).ok())
            .unwrap_or_else(|| request.url().to_string());
        request.with_dedup_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_with_jitter_keeps_delay_within_configured_range() {
        let base = Duration::from_millis(10);
        let jitter = Duration::from_millis(50);

        for _ in 0..100 {
            let delay = delay_with_jitter(base, Some(jitter));
            assert!(delay >= base);
            assert!(delay <= base + jitter);
        }
    }

    #[test]
    fn delay_with_jitter_returns_base_when_jitter_is_disabled() {
        let base = Duration::from_millis(10);

        assert_eq!(delay_with_jitter(base, None), base);
        assert_eq!(delay_with_jitter(base, Some(Duration::ZERO)), base);
    }
}
