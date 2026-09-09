use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use crate::config::{FetcherNodeConfig, RetrieverClientType};
use crate::traits::TorrentClientTrait;

pub trait FetcherDriverFactory: Send + Sync {
    fn client_type(&self) -> RetrieverClientType;
    fn create_client(&self, config: FetcherNodeConfig) -> Arc<dyn TorrentClientTrait>;
}

pub struct FetcherRegistry {
    drivers: RwLock<HashMap<RetrieverClientType, Box<dyn FetcherDriverFactory>>>,
}

impl Default for FetcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FetcherRegistry {
    pub fn new() -> Self {
        Self {
            drivers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, factory: Box<dyn FetcherDriverFactory>) {
        self.drivers.write().insert(factory.client_type(), factory);
    }

    pub fn create_client(&self, config: FetcherNodeConfig) -> anyhow::Result<Arc<dyn TorrentClientTrait>> {
        let drivers = self.drivers.read();
        let factory = drivers.get(&config.client_type)
            .ok_or_else(|| anyhow::anyhow!("No fetcher driver registered for client type: {:?}", config.client_type))?;
        Ok(factory.create_client(config))
    }
}
