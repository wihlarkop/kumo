use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use rand::Rng;
use tokio::sync::Mutex;

use crate::{
    frontier::{DeadLetterReason, Frontier, FrontierLeaseId},
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

#[derive(Debug, Clone)]
pub(crate) struct ScheduledRequest {
    pub(crate) queued: FrontierRequest,
    pub(crate) lease_id: Option<FrontierLeaseId>,
}

impl ScheduledRequest {
    pub(crate) fn new(queued: FrontierRequest, lease_id: Option<FrontierLeaseId>) -> Self {
        Self { queued, lease_id }
    }

    pub(crate) fn lease_id(&self) -> Option<&FrontierLeaseId> {
        self.lease_id.as_ref()
    }

    fn into_request(self) -> FrontierRequest {
        self.queued
    }
}

pub(crate) enum SchedulerPoll {
    Ready(Box<ScheduledRequest>),
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
            SchedulerPoll::Ready(scheduled) => Some(scheduled.into_request()),
            SchedulerPoll::Pending(_) | SchedulerPoll::Empty => None,
        }
    }

    pub(crate) async fn poll_ready(&self) -> SchedulerPoll {
        self.poll_next().await
    }

    pub async fn next_ready(&self) -> Option<FrontierRequest> {
        loop {
            match self.poll_next().await {
                SchedulerPoll::Ready(scheduled) => return Some(scheduled.into_request()),
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

    pub(crate) async fn ack(
        &self,
        scheduled: &ScheduledRequest,
    ) -> Result<(), crate::error::KumoError> {
        if let Some(lease_id) = scheduled.lease_id() {
            self.frontier.ack_lease(lease_id).await?;
        }
        Ok(())
    }

    pub(crate) async fn release(
        &self,
        scheduled: &ScheduledRequest,
    ) -> Result<(), crate::error::KumoError> {
        if let Some(lease_id) = scheduled.lease_id() {
            self.frontier.release_lease(lease_id).await?;
        }
        Ok(())
    }

    pub(crate) async fn dead_letter(
        &self,
        scheduled: &ScheduledRequest,
        reason: DeadLetterReason,
    ) -> Result<(), crate::error::KumoError> {
        if let Some(lease_id) = scheduled.lease_id() {
            self.frontier.dead_letter(lease_id, reason).await?;
        }
        Ok(())
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
            return self
                .frontier
                .next_ready_delay()
                .await
                .map_or(SchedulerPoll::Empty, SchedulerPoll::Pending);
        };

        let mut deferred = Vec::new();
        let mut shortest_wait: Option<Duration> = None;

        for _ in 0..queued_len {
            let Some(scheduled) = self.next_candidate().await else {
                break;
            };

            match self.classify_candidate(&scheduled.queued).await {
                CandidateState::Ready => {
                    self.requeue_deferred(deferred).await;
                    return SchedulerPoll::Ready(Box::new(scheduled));
                }
                CandidateState::Pending(wait) => {
                    shortest_wait = Some(shortest_wait.map_or(wait, |current| current.min(wait)));
                    deferred.push(scheduled);
                }
            }
        }

        self.requeue_deferred(deferred).await;

        if let Some(wait) = shortest_wait {
            SchedulerPoll::Pending(wait)
        } else {
            self.frontier
                .next_ready_delay()
                .await
                .map_or(SchedulerPoll::Empty, SchedulerPoll::Pending)
        }
    }

    async fn next_candidate(&self) -> Option<ScheduledRequest> {
        if self.frontier.supports_leases() {
            self.frontier
                .lease_request(Duration::from_secs(300))
                .await
                .map(|lease| {
                    let lease_id = lease.id().clone();
                    ScheduledRequest::new(lease.into_request(), Some(lease_id))
                })
        } else {
            self.frontier
                .pop_request()
                .await
                .map(|queued| ScheduledRequest::new(queued, None))
        }
    }

    async fn requeue_deferred(&self, deferred: Vec<ScheduledRequest>) {
        for scheduled in deferred {
            if scheduled.lease_id().is_some() {
                self.release(&scheduled).await.ok();
            } else {
                self.frontier.push_request_force(scheduled.queued).await;
            }
        }
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
