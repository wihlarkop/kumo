use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use kumo::{
    error::KumoError,
    extract::Response,
    fetch::{CachingFetcher, Fetcher, MockFetcher},
    middleware::FetchRequest,
};
use reqwest::Method;

fn req(url: &str) -> FetchRequest {
    FetchRequest::new(url, 0)
}

fn post_req(url: &str) -> FetchRequest {
    let mut request = FetchRequest::new(url, 0);
    request.method = Method::POST;
    request.body = Some(br#"{"page":1}"#.to_vec());
    request
}

#[derive(Clone)]
enum SequenceBody {
    Text(&'static str),
    Bytes(&'static [u8]),
}

#[derive(Clone)]
struct SequenceResponse {
    status: u16,
    body: SequenceBody,
}

struct SequenceFetcher {
    calls: Arc<AtomicUsize>,
    responses: Vec<SequenceResponse>,
}

impl SequenceFetcher {
    fn new(responses: Vec<SequenceResponse>) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            responses,
        }
    }

    fn calls(&self) -> Arc<AtomicUsize> {
        self.calls.clone()
    }
}

#[async_trait::async_trait]
impl Fetcher for SequenceFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<Response, KumoError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let response = self
            .responses
            .get(index)
            .or_else(|| self.responses.last())
            .expect("sequence fetcher needs at least one response");

        Ok(match response.body {
            SequenceBody::Text(body) => {
                Response::from_parts(request.url().to_string(), response.status, body)
            }
            SequenceBody::Bytes(body) => Response::from_bytes(
                request.url().to_string(),
                response.status,
                Bytes::from(body),
            ),
        })
    }
}

#[tokio::test]
async fn first_request_fetches_from_inner() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = MockFetcher::new().with_response("https://example.com", 200, "<h1>Hello</h1>");
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    let res = cf.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), Some("<h1>Hello</h1>"));
}

#[tokio::test]
async fn second_request_served_from_cache() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = MockFetcher::new().with_response("https://example.com", 200, "from network");
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    cf.fetch(&req("https://example.com")).await.unwrap();
    let res2 = cf.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res2.text(), Some("from network"));
}

#[tokio::test]
async fn cache_file_is_created_after_fetch() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = MockFetcher::new().with_response("https://example.com", 200, "body");
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    cf.fetch(&req("https://example.com")).await.unwrap();

    let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(files.len(), 1);
}

#[tokio::test]
async fn expired_entry_is_refetched() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = SequenceFetcher::new(vec![
        SequenceResponse {
            status: 200,
            body: SequenceBody::Text("cached"),
        },
        SequenceResponse {
            status: 200,
            body: SequenceBody::Text("refetched"),
        },
    ]);
    let cf = CachingFetcher::new(inner, tmp.path())
        .unwrap()
        .ttl(Duration::from_secs(0));

    cf.fetch(&req("https://example.com")).await.unwrap();
    let res = cf.fetch(&req("https://example.com")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), Some("refetched"));
}

#[tokio::test]
async fn cached_response_preserves_status_on_hit() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = SequenceFetcher::new(vec![
        SequenceResponse {
            status: 404,
            body: SequenceBody::Text("missing"),
        },
        SequenceResponse {
            status: 200,
            body: SequenceBody::Text("live"),
        },
    ]);
    let calls = inner.calls();
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    cf.fetch(&req("https://example.com/missing")).await.unwrap();
    let cached = cf.fetch(&req("https://example.com/missing")).await.unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(cached.status(), 404);
    assert_eq!(cached.text(), Some("missing"));
}

#[tokio::test]
async fn non_get_requests_bypass_cache() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = SequenceFetcher::new(vec![
        SequenceResponse {
            status: 200,
            body: SequenceBody::Text("post 1"),
        },
        SequenceResponse {
            status: 200,
            body: SequenceBody::Text("post 2"),
        },
    ]);
    let calls = inner.calls();
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    let first = cf
        .fetch(&post_req("https://example.com/search"))
        .await
        .unwrap();
    let second = cf
        .fetch(&post_req("https://example.com/search"))
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(first.text(), Some("post 1"));
    assert_eq!(second.text(), Some("post 2"));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn binary_responses_bypass_cache_writes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let inner = SequenceFetcher::new(vec![
        SequenceResponse {
            status: 200,
            body: SequenceBody::Bytes(b"\x89PNG first"),
        },
        SequenceResponse {
            status: 200,
            body: SequenceBody::Bytes(b"\x89PNG second"),
        },
    ]);
    let calls = inner.calls();
    let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

    let first = cf
        .fetch(&req("https://example.com/image.png"))
        .await
        .unwrap();
    let second = cf
        .fetch(&req("https://example.com/image.png"))
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(first.bytes(), b"\x89PNG first");
    assert_eq!(second.bytes(), b"\x89PNG second");
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}
