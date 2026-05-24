use kumo::error::{KumoError, KumoErrorKind};

#[cfg(feature = "persistence")]
use kumo::frontier::FileFrontier;

#[test]
fn error_helpers_preserve_context_in_display() {
    let parse = KumoError::parse_msg("bad selector");
    assert_eq!(
        parse.to_string(),
        "parse error - bad selector: bad selector"
    );

    let store = KumoError::store_msg("queue is invalid");
    assert_eq!(
        store.to_string(),
        "store error - queue is invalid: queue is invalid"
    );
}

#[test]
fn error_helpers_preserve_source_chain() {
    let source = std::io::Error::other("disk full");
    let err = KumoError::store("write queue", source);

    assert_eq!(err.to_string(), "store error - write queue: disk full");
    assert_eq!(
        std::error::Error::source(&err).map(ToString::to_string),
        Some("disk full".to_string())
    );
}

#[test]
fn error_kind_classifies_every_variant() {
    assert_eq!(
        KumoError::parse_msg("bad selector").kind(),
        KumoErrorKind::Parse
    );
    assert_eq!(
        KumoError::store_msg("bad disk").kind(),
        KumoErrorKind::Store
    );
    assert_eq!(
        KumoError::invalid_url("not a url").kind(),
        KumoErrorKind::InvalidUrl
    );
    assert_eq!(
        KumoError::DepthExceeded.kind(),
        KumoErrorKind::DepthExceeded
    );
    assert_eq!(
        KumoError::DomainNotAllowed("example.com".into()).kind(),
        KumoErrorKind::DomainNotAllowed
    );
    assert_eq!(KumoError::llm("bad json").kind(), KumoErrorKind::Llm);
    assert_eq!(
        KumoError::browser("browser closed").kind(),
        KumoErrorKind::Browser
    );
    assert_eq!(
        KumoError::HttpStatus {
            status: 503,
            url: "https://example.com".into(),
        }
        .kind(),
        KumoErrorKind::HttpStatus
    );
}

#[test]
fn error_kind_exports_stable_labels() {
    assert_eq!(KumoErrorKind::Fetch.as_str(), "fetch");
    assert_eq!(KumoErrorKind::Parse.as_str(), "parse");
    assert_eq!(KumoErrorKind::Store.as_str(), "store");
    assert_eq!(KumoErrorKind::InvalidUrl.as_str(), "invalid_url");
    assert_eq!(KumoErrorKind::DepthExceeded.as_str(), "depth_exceeded");
    assert_eq!(
        KumoErrorKind::DomainNotAllowed.as_str(),
        "domain_not_allowed"
    );
    assert_eq!(KumoErrorKind::Llm.as_str(), "llm");
    assert_eq!(KumoErrorKind::Browser.as_str(), "browser");
    assert_eq!(KumoErrorKind::HttpStatus.as_str(), "http_status");
    assert_eq!(KumoError::parse_msg("bad html").kind_label(), "parse");
}

#[test]
fn ergonomic_error_constructors_match_variants() {
    assert!(matches!(
        KumoError::invalid_url("not a url"),
        KumoError::InvalidUrl(value) if value == "not a url"
    ));
    assert!(matches!(
        KumoError::llm("model failed"),
        KumoError::Llm(value) if value == "model failed"
    ));
    assert!(matches!(
        KumoError::browser("page crashed"),
        KumoError::Browser(value) if value == "page crashed"
    ));
}

#[test]
fn http_status_helpers_expose_status_and_url_without_matching_fields() {
    let err = KumoError::http_status(503, "https://example.com/api");

    assert_eq!(err.status_code(), Some(503));
    assert_eq!(err.url(), Some("https://example.com/api"));
    assert_eq!(err.kind(), KumoErrorKind::HttpStatus);
}

#[cfg(feature = "persistence")]
#[test]
fn file_frontier_reports_malformed_queue_as_store_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("queue.json"), r#"{"not":"a queue"}"#).unwrap();

    let err = FileFrontier::open(dir.path()).unwrap_err();
    assert!(matches!(err, KumoError::Store { .. }));
    assert!(
        err.to_string().contains("parse queue.json"),
        "unexpected error: {err}"
    );
}
