use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
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
    assignments: HashMap<String, VecDeque<String>>,
}

#[derive(Debug, Default)]
struct ProxyHealthState {
    successes: u64,
    failures: u64,
    consecutive_failures: u64,
    cooldown_until: Option<Instant>,
    trial_in_flight: bool,
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

    fn pick(&self) -> Option<String> {
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
        if let Some(state) = health.stats.get_mut(proxy)
            && state.cooldown_until.is_some_and(|until| until <= now)
        {
            state.trial_in_flight = true;
        }
        Some(proxy.clone())
    }

    fn remember_assignment(&self, url: &str, proxy: &str) {
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        health
            .assignments
            .entry(url.to_string())
            .or_default()
            .push_back(proxy.to_string());
    }

    fn take_assignment(&self, url: &str) -> Option<String> {
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        let assigned = health
            .assignments
            .get_mut(url)
            .and_then(VecDeque::pop_front);
        if health.assignments.get(url).is_some_and(VecDeque::is_empty) {
            health.assignments.remove(url);
        }
        assigned
    }

    fn record_success(&self, proxy: &str) {
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        let state = health.stats.entry(proxy.to_string()).or_default();
        state.successes += 1;
        state.consecutive_failures = 0;
        state.cooldown_until = None;
        state.trial_in_flight = false;
    }

    fn record_failure(&self, proxy: &str) {
        let mut health = self.health.lock().expect("proxy health lock poisoned");
        let state = health.stats.entry(proxy.to_string()).or_default();
        state.failures += 1;
        state.consecutive_failures += 1;
        state.trial_in_flight = false;

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
        if let Some(proxy) = self.pick() {
            self.remember_assignment(request.url(), &proxy);
            request.proxy = Some(proxy);
        } else if !self.proxies.is_empty() {
            request.proxy = None;
        }
        Ok(())
    }

    async fn after_response(&self, response: &mut Response) -> Result<(), KumoError> {
        if let Some(proxy) = self.take_assignment(response.url()) {
            self.record_success(&proxy);
        }
        Ok(())
    }

    async fn on_error(&self, url: &str, _error: &KumoError) {
        if let Some(proxy) = self.take_assignment(url) {
            self.record_failure(&proxy);
        }
    }
}

fn is_cooling_down(state: Option<&ProxyHealthState>, now: Instant) -> bool {
    state
        .and_then(|state| state.cooldown_until)
        .is_some_and(|until| until > now)
}

fn is_unavailable(state: Option<&ProxyHealthState>, now: Instant) -> bool {
    is_cooling_down(state, now) || state.is_some_and(|state| state.trial_in_flight)
}

fn circuit_state(state: Option<&ProxyHealthState>, now: Instant) -> ProxyCircuitState {
    match state.and_then(|state| state.cooldown_until) {
        Some(until) if until > now => ProxyCircuitState::Open,
        Some(_) => ProxyCircuitState::Recovering,
        None => ProxyCircuitState::Healthy,
    }
}
