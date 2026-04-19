use thiserror::Error;

#[derive(Debug, Error)]
pub enum KumoError {
    #[error("fetch error: {0}")]
    Fetch(#[from] reqwest::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("store error: {0}")]
    Store(String),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("max crawl depth exceeded")]
    DepthExceeded,

    #[error("domain not allowed: {0}")]
    DomainNotAllowed(String),

    #[error("llm error: {0}")]
    Llm(String),

    #[error("browser error: {0}")]
    Browser(String),

    /// Returned by `StatusRetry` middleware when the response status code is
    /// in the retry set. Triggers the engine's exponential-backoff retry loop.
    #[error("HTTP {status} from {url}")]
    HttpStatus { status: u16, url: String },
}

/// Determines what the engine does when Spider::parse or a fetch fails.
#[derive(Debug, Clone)]
pub enum ErrorPolicy {
    /// Skip this URL and continue crawling. (default)
    Skip,
    /// Abort the entire crawl immediately.
    Abort,
    /// Retry this URL up to N more times.
    Retry(u32),
}
