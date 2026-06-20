use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;

use crate::{error::KumoError, extract::Response};

use super::{FetchRequest, Middleware, RotationStrategy};

const DEFAULT_FAILURE_THRESHOLD: u64 = 3;
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(60);

/// Middleware that assigns a proxy URL to each request, rotating through a pool.
///
/// The selected proxy URL is written to `request.proxy`; `HttpFetcher` picks it up
/// and routes the request through the specified proxy. Proxies are tracked by URL:
/// successful responses reset consecutive failures, while failed request attempts
/// increment failure counters. After repeated failures, a proxy is temporarily
/// skipped for a cooldown period.
///
/// ## Cookie isolation
///
/// Each proxy gets its own `reqwest::Client` with an independent cookie jar.
/// This is intentional for anonymity - requests through proxy A and proxy B
/// won't share session cookies. If you need shared cookies across proxies,
/// implement a custom `Fetcher`.
///
/// Proxy URLs follow reqwest's format: `"http://user:pass@host:port"` or
/// `"socks5://host:port"`.
///
/// # Examples
/// ```rust,ignore
/// ProxyRotator::new(vec![
///     "http://user:pass@proxy1.example.com:8080",
///     "http://proxy2.example.com:8080",
/// ])
///
/// ProxyRotator::random(vec!["socks5://p1:1080", "http://p2:8080"])
/// ```
#[derive(Clone)]
pub struct ProxyRotator {
    proxies: Vec<String>,
    strategy: RotationStrategy,
    health: Arc<Mutex<ProxyHealth>>,
    next_assignment_id: Arc<AtomicU64>,
    cooldown: Option<ProxyCooldown>,
}

/// Circuit-breaker state for one proxy URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyCircuitState {
    /// The proxy circuit is closed and the proxy is selectable.
    Healthy,
    /// The proxy circuit is open and the proxy is skipped until recovery.
    Open,
    /// The recovery period elapsed and the next selection is a trial request.
    Recovering,
}

/// Point-in-time health counters for one proxy URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyHealthSnapshot {
    pub proxy: String,
    pub successes: u64,
    pub failures: u64,
    pub consecutive_failures: u64,
    pub cooling_down: bool,
    pub cooldown_remaining: Option<Duration>,
}

/// Point-in-time circuit-breaker state for one proxy URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyCircuitSnapshot {
    pub proxy: String,
    pub successes: u64,
    pub failures: u64,
    pub consecutive_failures: u64,
    pub circuit_state: ProxyCircuitState,
    pub cooling_down: bool,
    pub cooldown_remaining: Option<Duration>,
}

#[derive(Debug, Clone, Copy)]
struct ProxyCooldown {
    failure_threshold: u64,
    duration: Duration,
}

#[derive(Debug, Default)]
struct ProxyHealth {
    stats: HashMap<String, ProxyHealthState>,
    assignments: HashMap<u64, ProxyAssignment>,
}

#[derive(Debug)]
struct ProxyAssignment {
    proxy: String,
    trial_generation: Option<u64>,
}

#[derive(Debug, Default)]
struct ProxyHealthState {
    successes: u64,
    failures: u64,
    consecutive_failures: u64,
    cooldown_until: Option<Instant>,
    trial_generation: u64,
    active_trial: Option<u64>,
}

impl ProxyRotator {
    /// Rotate through `proxies` in round-robin order.
    pub fn new(proxies: Vec<impl Into<String>>) -> Self {
        Self::with_strategy(proxies, RotationStrategy::round_robin())
    }

    /// Pick a proxy pseudo-randomly on each request.
    pub fn random(proxies: Vec<impl Into<String>>) -> Self {
        Self::with_strategy(proxies, RotationStrategy::random())
    }

    fn with_strategy(proxies: Vec<impl Into<String>>, strategy: RotationStrategy) -> Self {
        Self {
            proxies: proxies.into_iter().map(Into::into).collect(),
            strategy,
            health: Arc::new(Mutex::new(ProxyHealth::default())),
            next_assignment_id: Arc::new(AtomicU64::new(1)),
            cooldown: Some(ProxyCooldown {
                failure_threshold: DEFAULT_FAILURE_THRESHOLD,
                duration: DEFAULT_COOLDOWN,
            }),
        }
    }

    /// Configure how many consecutive failed attempts trigger cooldown.
    ///
    /// The threshold is clamped to at least one failure. By default, proxies
    /// cool down for 60 seconds after three consecutive failed request attempts.
    pub fn cooldown_after(mut self, failure_threshold: u64, duration: Duration) -> Self {
        self.cooldown = Some(ProxyCooldown {
            failure_threshold: failure_threshold.max(1),
            duration,
        });
        self
    }

    /// Disable cooldown while retaining health counters.
    pub fn without_cooldown(mut self) -> Self {
        self.cooldown = None;
        self
    }

    /// Return health counters for every configured proxy URL.
    pub fn health(&self) -> Vec<ProxyHealthSnapshot> {
        let now = Instant::now();
        let health = self.health.lock().expect("proxy health lock poisoned");

        self.proxies
            .iter()
            .map(|proxy| {
                let state = health.stats.get(proxy);
                let cooldown_remaining = state.and_then(|state| {
                    state
                        .cooldown_until
                        .and_then(|until| until.checked_duration_since(now))
                });

                ProxyHealthSnapshot {
                    proxy: proxy.clone(),
                    successes: state.map_or(0, |state| state.successes),
                    failures: state.map_or(0, |state| state.failures),
                    consecutive_failures: state.map_or(0, |state| state.consecutive_failures),
                    cooling_down: cooldown_remaining.is_some(),
                    cooldown_remaining,
                }
            })
            .collect()
    }

    /// Return circuit-breaker state and health counters for every configured proxy URL.
    pub fn circuit_health(&self) -> Vec<ProxyCircuitSnapshot> {
        let now = Instant::now();
        let health = self.health.lock().expect("proxy health lock poisoned");

        self.proxies
            .iter()
            .map(|proxy| {
                let state = health.stats.get(proxy);
                let cooldown_remaining = state.and_then(|state| {
                    state
                        .cooldown_until
                        .and_then(|until| until.checked_duration_since(now))
                });

                ProxyCircuitSnapshot {
                    proxy: proxy.clone(),
                    successes: state.map_or(0, |state| state.successes),
                    failures: state.map_or(0, |state| state.failures),
                    consecutive_failures: state.map_or(0, |state| state.consecutive_failures),
                    circuit_state: circuit_state(state, now),
                    cooling_down: cooldown_remaining.is_some(),
                    cooldown_remaining,
                }
            })
            .collect()
    }

    fn pick(&self) -> Option<ProxyAssignment> {
        if self.proxies.is_empty() {
            return None;
        }

        let mut health = self.health.lock().expect("proxy health lock poisoned");
        let now = Instant::now();
        let eligible = self
            .proxies
            .iter()
            .enumerate()
            .filter_map(|(index, proxy)| {
                if is_unavailable(health.stats.get(proxy), now) {
                    None
                } else {
                    Some(index)
                }
            })
            .collect::<Vec<_>>();

        if eligible.is_empty() {
            return None;
        }

        let picked = eligible[self.strategy.pick_index(eligible.len())];
        let proxy = &self.proxies[picked];
        let trial_generation = health.stats.get_mut(proxy).and_then(|state| {
            if state.cooldown_until.is_some_and(|until| until <= now) {
                state.trial_generation = state.trial_generation.wrapping_add(1);
                state.active_trial = Some(state.trial_generation);
                Some(state.trial_generation)
            } else {
                None
            }
        });
        Some(ProxyAssignment {
            proxy: proxy.clone(),
            trial_generation,
        })
    }

    fn remember_assignment(&self, assignment_id: u64, assignment: ProxyAssignment) {
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        health.assignments.insert(assignment_id, assignment);
    }

    fn take_assignment(&self, request: &FetchRequest) -> Option<ProxyAssignment> {
        let assignment_id = request.proxy_assignment_id()?;
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        health.assignments.remove(&assignment_id)
    }

    fn record_success(&self, assignment: ProxyAssignment) {
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        let state = health.stats.entry(assignment.proxy).or_default();
        state.successes += 1;
        if !assignment_resolves_circuit(state, assignment.trial_generation) {
            return;
        }
        state.consecutive_failures = 0;
        state.cooldown_until = None;
        state.active_trial = None;
    }

    fn record_failure(&self, assignment: ProxyAssignment) {
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        let state = health.stats.entry(assignment.proxy).or_default();
        state.failures += 1;
        if !assignment_resolves_circuit(state, assignment.trial_generation) {
            return;
        }
        state.consecutive_failures += 1;
        state.active_trial = None;

        if let Some(cooldown) = self.cooldown
            && state.consecutive_failures >= cooldown.failure_threshold
        {
            state.cooldown_until = Some(Instant::now() + cooldown.duration);
        }
    }
}

#[async_trait]
impl Middleware for ProxyRotator {
    async fn before_request(&self, request: &mut FetchRequest) -> Result<(), KumoError> {
        request.set_proxy_assignment_id(None);
        if let Some(assignment) = self.pick() {
            let assignment_id = self.next_assignment_id.fetch_add(1, Ordering::Relaxed);
            request.proxy = Some(assignment.proxy.clone());
            self.remember_assignment(assignment_id, assignment);
            request.set_proxy_assignment_id(Some(assignment_id));
        } else if !self.proxies.is_empty() {
            request.proxy = None;
        }
        Ok(())
    }

    async fn after_response_with_request(
        &self,
        request: &FetchRequest,
        _response: &mut Response,
    ) -> Result<(), KumoError> {
        if let Some(assignment) = self.take_assignment(request) {
            self.record_success(assignment);
        }
        Ok(())
    }

    async fn on_fetch_error(&self, request: &FetchRequest, _error: &KumoError) {
        if let Some(assignment) = self.take_assignment(request) {
            self.record_failure(assignment);
        }
    }
}

fn is_cooling_down(state: Option<&ProxyHealthState>, now: Instant) -> bool {
    state
        .and_then(|state| state.cooldown_until)
        .is_some_and(|until| until > now)
}

fn is_unavailable(state: Option<&ProxyHealthState>, now: Instant) -> bool {
    is_cooling_down(state, now) || state.is_some_and(|state| state.active_trial.is_some())
}

fn assignment_resolves_circuit(state: &ProxyHealthState, trial_generation: Option<u64>) -> bool {
    match trial_generation {
        Some(generation) => state.active_trial == Some(generation),
        None => state.active_trial.is_none(),
    }
}

fn circuit_state(state: Option<&ProxyHealthState>, now: Instant) -> ProxyCircuitState {
    match state.and_then(|state| state.cooldown_until) {
        Some(until) if until > now => ProxyCircuitState::Open,
        Some(_) => ProxyCircuitState::Recovering,
        None => ProxyCircuitState::Healthy,
    }
}
