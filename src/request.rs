use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    Method,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A request a spider wants the crawler to schedule.
///
/// Use [`Output::follow`](crate::spider::Output::follow) for the common case,
/// or `CrawlRequest` when you need priority, headers, metadata, or dedup
/// control for a followed URL.
#[derive(Debug, Clone)]
pub struct CrawlRequest {
    url: String,
    method: Method,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
    priority: i32,
    meta: HashMap<String, Value>,
    dont_filter: bool,
    dedup_key: Option<String>,
}

impl CrawlRequest {
    /// Schedule a GET request for `url`.
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            priority: 0,
            meta: HashMap::new(),
            dont_filter: false,
            dedup_key: None,
        }
    }

    /// Schedule a POST request for `url`.
    pub fn post(url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        Self::get(url).method(Method::POST).body(body)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn method_ref(&self) -> &Method {
        &self.method
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn body_bytes(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    pub fn priority_value(&self) -> i32 {
        self.priority
    }

    pub fn meta_value(&self, key: &str) -> Option<&Value> {
        self.meta.get(key)
    }

    pub fn dont_filter_enabled(&self) -> bool {
        self.dont_filter
    }

    pub(crate) fn dedup_key(&self) -> &str {
        self.dedup_key.as_deref().unwrap_or(&self.url)
    }

    pub fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Higher priority requests are crawled before lower priority requests by
    /// the default in-memory frontier.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn meta(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.meta.insert(key.into(), value.into());
        self
    }

    /// Bypass URL deduplication for this request.
    pub fn dont_filter(mut self, value: bool) -> Self {
        self.dont_filter = value;
        self
    }

    pub(crate) fn with_dedup_key(mut self, key: impl Into<String>) -> Self {
        self.dedup_key = Some(key.into());
        self
    }
}

impl From<String> for CrawlRequest {
    fn from(url: String) -> Self {
        Self::get(url)
    }
}

impl From<&str> for CrawlRequest {
    fn from(url: &str) -> Self {
        Self::get(url)
    }
}

/// A crawl request plus engine-owned scheduling state stored by a frontier.
///
/// Most spiders only need [`CrawlRequest`]. This type is exposed for custom
/// [`Frontier`](crate::frontier::Frontier) implementations.
#[derive(Debug, Clone)]
pub struct FrontierRequest {
    pub(crate) request: CrawlRequest,
    pub(crate) depth: usize,
    pub(crate) retry_count: u32,
    pub(crate) sequence: u64,
    pub(crate) scheduled_at: Option<SystemTime>,
}

impl FrontierRequest {
    /// Build a frontier record from a request, crawl depth, and retry count.
    pub fn new(request: CrawlRequest, depth: usize, retry_count: u32) -> Self {
        Self {
            request,
            depth,
            retry_count,
            sequence: REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            scheduled_at: None,
        }
    }

    /// The request to fetch.
    pub fn request(&self) -> &CrawlRequest {
        &self.request
    }

    /// The crawl depth assigned to this request.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Number of times this request has already been retried by the engine.
    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }

    /// Schedule this request to become eligible after `delay`.
    pub fn scheduled_after(mut self, delay: Duration) -> Self {
        self.scheduled_at = Some(SystemTime::now() + delay);
        self
    }

    /// The earliest time this request should be dispatched.
    pub fn scheduled_at(&self) -> Option<SystemTime> {
        self.scheduled_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredCrawlRequest {
    url: String,
    method: String,
    headers: Vec<StoredHeader>,
    body: Option<Vec<u8>>,
    priority: i32,
    meta: HashMap<String, Value>,
    dont_filter: bool,
    dedup_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredHeader {
    name: String,
    value: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredFrontierRequest {
    request: StoredCrawlRequest,
    depth: usize,
    retry_count: u32,
    scheduled_at_ms: Option<u64>,
}

impl From<&CrawlRequest> for StoredCrawlRequest {
    fn from(request: &CrawlRequest) -> Self {
        let headers = request
            .headers
            .iter()
            .map(|(name, value)| StoredHeader {
                name: name.as_str().to_string(),
                value: value.as_bytes().to_vec(),
            })
            .collect();

        Self {
            url: request.url.clone(),
            method: request.method.as_str().to_string(),
            headers,
            body: request.body.clone(),
            priority: request.priority,
            meta: request.meta.clone(),
            dont_filter: request.dont_filter,
            dedup_key: request.dedup_key.clone(),
        }
    }
}

impl TryFrom<StoredCrawlRequest> for CrawlRequest {
    type Error = &'static str;

    fn try_from(stored: StoredCrawlRequest) -> Result<Self, Self::Error> {
        let method = Method::from_bytes(stored.method.as_bytes()).map_err(|_| "invalid method")?;
        let mut headers = HeaderMap::new();
        for header in stored.headers {
            let name = HeaderName::from_bytes(header.name.as_bytes())
                .map_err(|_| "invalid header name")?;
            let value =
                HeaderValue::from_bytes(&header.value).map_err(|_| "invalid header value")?;
            headers.insert(name, value);
        }

        Ok(Self {
            url: stored.url,
            method,
            headers,
            body: stored.body,
            priority: stored.priority,
            meta: stored.meta,
            dont_filter: stored.dont_filter,
            dedup_key: stored.dedup_key,
        })
    }
}

impl From<&FrontierRequest> for StoredFrontierRequest {
    fn from(queued: &FrontierRequest) -> Self {
        Self {
            request: StoredCrawlRequest::from(&queued.request),
            depth: queued.depth,
            retry_count: queued.retry_count,
            scheduled_at_ms: queued.scheduled_at.and_then(system_time_to_ms),
        }
    }
}

impl TryFrom<StoredFrontierRequest> for FrontierRequest {
    type Error = &'static str;

    fn try_from(stored: StoredFrontierRequest) -> Result<Self, Self::Error> {
        let mut request = Self::new(
            CrawlRequest::try_from(stored.request)?,
            stored.depth,
            stored.retry_count,
        );
        request.scheduled_at = stored
            .scheduled_at_ms
            .map(|ms| UNIX_EPOCH + Duration::from_millis(ms));
        Ok(request)
    }
}

fn system_time_to_ms(time: SystemTime) -> Option<u64> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(duration.as_millis()).ok()
}
