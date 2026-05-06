use crate::{
    engine::CrawlStats,
    error::{ErrorPolicy, KumoError},
    extract::Response,
    spider::{Output, Spider},
};

pub(super) struct ErasedOutput {
    pub(super) items: Vec<serde_json::Value>,
    pub(super) follow: Vec<String>,
}

#[async_trait::async_trait]
pub(super) trait ErasedSpider: Send + Sync {
    fn name(&self) -> &str;
    fn start_urls(&self) -> Vec<String>;
    async fn parse_erased(&self, response: &Response) -> Result<ErasedOutput, KumoError>;
    fn on_error(&self, url: &str, err: &KumoError) -> ErrorPolicy;
    fn max_depth(&self) -> Option<usize>;
    fn allowed_domains(&self) -> Vec<&str>;
    async fn open(&self) -> Result<(), KumoError>;
    async fn close(&self, stats: &CrawlStats) -> Result<(), KumoError>;
}

pub(super) struct SpiderErased<S>(pub(super) S);

#[async_trait::async_trait]
impl<S: Spider + 'static> ErasedSpider for SpiderErased<S> {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn start_urls(&self) -> Vec<String> {
        self.0.start_urls()
    }

    async fn parse_erased(&self, response: &Response) -> Result<ErasedOutput, KumoError> {
        let output: Output<S::Item> = self.0.parse(response).await?;
        let items = output
            .items
            .into_iter()
            .map(|item| {
                serde_json::to_value(item).map_err(|e| KumoError::parse("item serialization", e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ErasedOutput {
            items,
            follow: output.follow,
        })
    }

    fn on_error(&self, url: &str, err: &KumoError) -> ErrorPolicy {
        self.0.on_error(url, err)
    }

    fn max_depth(&self) -> Option<usize> {
        self.0.max_depth()
    }

    fn allowed_domains(&self) -> Vec<&str> {
        self.0.allowed_domains()
    }

    async fn open(&self) -> Result<(), KumoError> {
        self.0.open().await
    }

    async fn close(&self, stats: &CrawlStats) -> Result<(), KumoError> {
        self.0.close(stats).await
    }
}
