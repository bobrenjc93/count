pub mod compression;
pub mod storage;
pub mod query;
pub mod error;
pub mod cluster;
pub mod distributed;

use error::CountResult;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataPoint {
    pub timestamp: u64,
    pub value: f64,
}

impl DataPoint {
    pub fn new(timestamp: u64, value: f64) -> Self {
        Self { timestamp, value }
    }
}

#[derive(Debug, Clone)]
pub struct SeriesKey(pub String);

impl From<&str> for SeriesKey {
    fn from(s: &str) -> Self {
        SeriesKey(s.to_string())
    }
}

impl From<String> for SeriesKey {
    fn from(s: String) -> Self {
        SeriesKey(s)
    }
}

#[derive(Debug, Clone)]
pub struct CountConfig {
    pub memory_buffer_size: usize,
    pub flush_interval_seconds: u64,
    pub data_dir: String,
    // Cluster configuration
    pub cluster_enabled: bool,
    pub node_id: Option<u32>,
    pub bind_address: Option<String>,
    pub seed_nodes: Vec<String>,
    pub replication_factor: usize,
}

impl Default for CountConfig {
    fn default() -> Self {
        Self {
            memory_buffer_size: 10000,
            flush_interval_seconds: 300, // 5 minutes
            data_dir: "./count_data".to_string(),
            cluster_enabled: false,
            node_id: None,
            bind_address: None,
            seed_nodes: Vec::new(),
            replication_factor: 2,
        }
    }
}

impl CountConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        
        if let Ok(cluster_enabled) = std::env::var("CLUSTER_ENABLED") {
            config.cluster_enabled = cluster_enabled.parse().unwrap_or(false);
        }
        
        if let Ok(node_id_str) = std::env::var("NODE_ID") {
            config.node_id = node_id_str.parse().ok();
        }
        
        if let Ok(bind_addr) = std::env::var("BIND_ADDRESS") {
            config.bind_address = Some(bind_addr);
        }
        
        if let Ok(seed_nodes_str) = std::env::var("CLUSTER_NODES") {
            config.seed_nodes = seed_nodes_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        
        if let Ok(repl_factor_str) = std::env::var("REPLICATION_FACTOR") {
            config.replication_factor = repl_factor_str.parse().unwrap_or(2);
        }
        
        if let Ok(data_dir) = std::env::var("DATA_DIR") {
            config.data_dir = data_dir;
        }
        
        config
    }
}

pub struct CountDB {
    #[allow(dead_code)]
    config: CountConfig,
    storage: Arc<RwLock<storage::StorageEngine>>,
}

impl CountDB {
    pub async fn new(config: CountConfig) -> CountResult<Self> {
        let storage = storage::StorageEngine::new(&config).await?;
        
        Ok(Self {
            config,
            storage: Arc::new(RwLock::new(storage)),
        })
    }

    pub async fn insert(&self, series: SeriesKey, point: DataPoint) -> CountResult<()> {
        let mut storage = self.storage.write().await;
        storage.insert(series, point).await
    }

    pub async fn query_range(
        &self,
        series: SeriesKey,
        start_time: u64,
        end_time: u64,
    ) -> CountResult<Vec<DataPoint>> {
        let storage = self.storage.read().await;
        storage.query_range(series, start_time, end_time).await
    }

    pub async fn query_aggregated(
        &self,
        series: SeriesKey,
        start_time: u64,
        end_time: u64,
        aggregation: query::AggregationType,
    ) -> CountResult<f64> {
        let storage = self.storage.read().await;
        storage.query_aggregated(series, start_time, end_time, aggregation).await
    }

    pub async fn shutdown(&self) -> CountResult<()> {
        let mut storage = self.storage.write().await;
        storage.shutdown().await
    }
}