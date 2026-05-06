use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

use crate::error::KumoError;

pub(super) struct ChannelStore {
    tx: tokio::sync::mpsc::Sender<serde_json::Value>,
    cancelled: Arc<AtomicBool>,
}

impl ChannelStore {
    pub(super) fn new(
        tx: tokio::sync::mpsc::Sender<serde_json::Value>,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self { tx, cancelled }
    }
}

#[async_trait::async_trait]
impl crate::store::ItemStore for ChannelStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        if self.tx.send(item.clone()).await.is_err() {
            self.cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}

/// An async stream of scraped items returned by [`CrawlEngine::stream`](super::CrawlEngine::stream).
///
/// Implements [`tokio_stream::Stream`]`<Item = serde_json::Value>`.
/// Use [`tokio_stream::StreamExt::next`] to consume items one by one.
///
/// Dropping this stream closes the channel, which causes the background
/// crawl engine to stop gracefully on its next attempted send.
pub struct ItemStream {
    inner: tokio_stream::wrappers::ReceiverStream<serde_json::Value>,
}

impl ItemStream {
    pub(super) fn new(rx: tokio::sync::mpsc::Receiver<serde_json::Value>) -> Self {
        Self {
            inner: tokio_stream::wrappers::ReceiverStream::new(rx),
        }
    }
}

impl tokio_stream::Stream for ItemStream {
    type Item = serde_json::Value;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}
