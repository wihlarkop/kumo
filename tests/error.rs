use kumo::error::KumoError;

#[cfg(feature = "persistence")]
use kumo::frontier::FileFrontier;

#[test]
fn error_helpers_preserve_context_in_display() {
    let parse = KumoError::parse_msg("bad selector");
    assert_eq!(
        parse.to_string(),
        "parse error — bad selector: bad selector"
    );

    let store = KumoError::store_msg("queue is invalid");
    assert_eq!(
        store.to_string(),
        "store error — queue is invalid: queue is invalid"
    );
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
