use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

use crate::{
    frontier::Frontier,
    request::{CrawlRequest, FrontierRequest},
};

use super::{domain::domain_key, fingerprint::FingerprintPolicy, policy::PolitenessPolicy};

#[derive(Debug, Default)]
struct DomainState {
    in_flight: usize,
    next_available_at: Option<Instant>,
}

pub struct CrawlScheduler {
    frontier: Arc<dyn Frontier>,
    policy: PolitenessPolicy,
    fingerprint_policy: FingerprintPolicy,
    domains: Mutex<HashMap<String, DomainState>>,
}

enum SchedulerPoll {
    Ready(Box<FrontierRequest>),
    Pending(Duration),
    Empty,
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
        let Some(domain) = domain_key(queued.request().url()) else {
            return;
        };

        let mut domains = self.domains.lock().await;
        let state = domains.entry(domain.clone()).or_default();
        state.in_flight = state.in_flight.saturating_sub(1);
        if let Some(delay) = self.policy.policy_for(&domain).delay() {
            state.next_available_at = Some(Instant::now() + delay);
        }
    }

    async fn poll_next(&self) -> SchedulerPoll {
        let Some(queued) = self.frontier.pop_request().await else {
            return SchedulerPoll::Empty;
        };
        if let Some(scheduled_at) = queued.scheduled_at()
            && let Ok(wait) = scheduled_at.duration_since(std::time::SystemTime::now())
        {
            self.frontier.push_request_force(queued).await;
            return SchedulerPoll::Pending(wait);
        }

        let Some(domain) = domain_key(queued.request().url()) else {
            return SchedulerPoll::Ready(Box::new(queued));
        };

        let wait = {
            let mut domains = self.domains.lock().await;
            let state = domains.entry(domain.clone()).or_default();
            let domain_policy = self.policy.policy_for(&domain);

            if state.in_flight >= domain_policy.concurrency() {
                Some(Duration::from_millis(10))
            } else if let Some(next) = state.next_available_at {
                next.checked_duration_since(Instant::now())
            } else {
                state.in_flight += 1;
                None
            }
        };

        if let Some(wait) = wait {
            self.frontier.push_request_force(queued).await;
            return SchedulerPoll::Pending(wait);
        }

        SchedulerPoll::Ready(Box::new(queued))
    }

    fn apply_fingerprint(&self, request: CrawlRequest) -> CrawlRequest {
        if request.dont_filter_enabled() {
            return request;
        }

        let key = self
            .fingerprint_policy
            .fingerprint(request.url())
            .unwrap_or_else(|_| request.url().to_string());
        request.with_dedup_key(key)
    }
}
