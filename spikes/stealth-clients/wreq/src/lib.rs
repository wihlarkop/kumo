use async_trait::async_trait;
use bytes::Bytes;
use reqwest::header::HeaderMap;
use std::time::Duration;
use wreq::{Client, Method};
use wreq_util::Emulation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpikeStealthProfile {
    Chrome131,
    Firefox128,
    Safari18,
    Edge127,
}

#[derive(Debug, Default)]
pub struct ProbeRequest {
    pub url: String,
    pub method: reqwest::Method,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
    pub proxy: Option<String>,
}

#[derive(Debug)]
pub struct ProbeResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub elapsed: Duration,
}

#[async_trait]
pub trait ProbeAdapter {
    async fn fetch(&self, request: &ProbeRequest) -> Result<ProbeResponse, String>;
}

pub struct WreqAdapter {
    profile: SpikeStealthProfile,
    client: Client,
}

impl WreqAdapter {
    pub fn new(profile: SpikeStealthProfile) -> Result<Self, String> {
        let client = Client::builder()
            .emulation(to_wreq_emulation(profile))
            .cookie_store(true)
            .build()
            .map_err(|e| format!("wreq client: {e}"))?;

        Ok(Self { profile, client })
    }
}

fn to_wreq_emulation(profile: SpikeStealthProfile) -> Emulation {
    match profile {
        SpikeStealthProfile::Chrome131 => Emulation::Chrome131,
        SpikeStealthProfile::Firefox128 => Emulation::Firefox128,
        SpikeStealthProfile::Safari18 => Emulation::Safari18,
        SpikeStealthProfile::Edge127 => Emulation::Edge127,
    }
}

fn to_wreq_method(method: &reqwest::Method) -> Result<Method, String> {
    Method::from_bytes(method.as_str().as_bytes()).map_err(|e| format!("wreq method: {e}"))
}

#[async_trait]
impl ProbeAdapter for WreqAdapter {
    async fn fetch(&self, request: &ProbeRequest) -> Result<ProbeResponse, String> {
        let mut builder = self
            .client
            .request(to_wreq_method(&request.method)?, request.url.as_str());

        if let Some(proxy_url) = &request.proxy {
            builder = builder.proxy(proxy_url);
        }

        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.to_str().unwrap_or(""));
        }

        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }

        let start = std::time::Instant::now();
        let response = builder
            .send()
            .await
            .map_err(|e| format!("wreq fetch: {e}"))?;
        let status = response.status().as_u16();
        let headers = to_reqwest_headers(response.headers());
        let body = response
            .bytes()
            .await
            .map_err(|e| format!("wreq body: {e}"))?;

        Ok(ProbeResponse {
            status,
            headers,
            body,
            elapsed: start.elapsed(),
        })
    }
}

fn to_reqwest_headers(source: &wreq::header::HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in source {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            headers.insert(name, value);
        }
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_all_profile_clients() {
        for profile in [
            SpikeStealthProfile::Chrome131,
            SpikeStealthProfile::Firefox128,
            SpikeStealthProfile::Safari18,
            SpikeStealthProfile::Edge127,
        ] {
            WreqAdapter::new(profile).expect("wreq client should build");
        }
    }

    #[tokio::test]
    #[ignore = "requires network access and native TLS build dependencies"]
    async fn can_fetch_tls_fingerprint_probe() {
        let adapter = WreqAdapter::new(SpikeStealthProfile::Chrome131).unwrap();
        let request = ProbeRequest {
            url: "https://tls.peet.ws/api/all".to_string(),
            method: reqwest::Method::GET,
            ..ProbeRequest::default()
        };
        let response = adapter.fetch(&request).await.unwrap();
        assert_eq!(response.status, 200);
        assert!(!response.body.is_empty());
    }
}
