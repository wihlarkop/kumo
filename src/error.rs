use thiserror::Error;

#[derive(Debug, Error)]
pub enum KumoError {
    #[error("fetch error: {0}")]
    Fetch(#[from] reqwest::Error),

    #[error("parse error — {context}: {source}")]
    Parse {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("store error — {context}: {source}")]
    Store {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

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

    /// Returned by `StatusRetry` middleware when the response status code matches
    /// the retry set. Triggers the engine's exponential-backoff retry loop.
    #[error("HTTP {status} from {url}")]
    HttpStatus { status: u16, url: String },
}

/// Thin wrapper so plain `String` messages can be boxed as `dyn Error`.
#[derive(Debug)]
struct Msg(String);
impl std::fmt::Display for Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Msg {}

impl KumoError {
    /// Construct a `Parse` variant from a real source error.
    pub fn parse(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Parse { context: context.into(), source: Box::new(source) }
    }

    /// Construct a `Parse` variant from a plain message (no source).
    pub fn parse_msg(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        Self::Parse { context: msg.clone(), source: Box::new(Msg(msg)) }
    }

    /// Construct a `Store` variant from a real source error.
    pub fn store(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Store { context: context.into(), source: Box::new(source) }
    }

    /// Construct a `Store` variant from a plain message (no source).
    pub fn store_msg(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        Self::Store { context: msg.clone(), source: Box::new(Msg(msg)) }
    }
}

/// Determines what the engine does when Spider::parse or a fetch fails.
#[derive(Debug, Clone)]
pub enum ErrorPolicy {
    /// Skip this URL and continue crawling. (default)
    Skip,
    /// Abort the entire crawl immediately.
    Abort,
    /// Retry this URL up to N more times (via the frontier).
    Retry(u32),
}
