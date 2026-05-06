use kumo::{error::KumoError, store::ItemStore};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct VecStore {
    items: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl VecStore {
    pub fn collected(&self) -> Vec<serde_json::Value> {
        self.items.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ItemStore for VecStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        self.items.lock().unwrap().push(item.clone());
        Ok(())
    }
}
