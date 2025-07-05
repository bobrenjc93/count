pub mod compression;
pub mod storage;
pub mod query;
pub mod error;

use error::CountResult;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
}

impl Default for CountConfig {
    fn default() -> Self {
        Self {
            memory_buffer_size: 10000,
            flush_interval_seconds: 300, // 5 minutes
            data_dir: "./count_data".to_string(),
        }
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