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
    // S3 archival configuration  
    pub s3_enabled: bool,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_prefix: Option<String>,
    pub archival_age_days: u32,
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
            s3_enabled: false,
            s3_bucket: None,
            s3_region: None,
            s3_prefix: None,
            archival_age_days: 14, // 2 weeks
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

        if let Ok(s3_enabled) = std::env::var("S3_ENABLED") {
            config.s3_enabled = s3_enabled.parse().unwrap_or(false);
        }
        
        if let Ok(s3_bucket) = std::env::var("S3_BUCKET") {
            config.s3_bucket = Some(s3_bucket);
        }
        
        if let Ok(s3_region) = std::env::var("S3_REGION") {
            config.s3_region = Some(s3_region);
        }
        
        if let Ok(s3_prefix) = std::env::var("S3_PREFIX") {
            config.s3_prefix = Some(s3_prefix);
        }
        
        if let Ok(archival_age_str) = std::env::var("ARCHIVAL_AGE_DAYS") {
            config.archival_age_days = archival_age_str.parse().unwrap_or(14);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_default_config() {
        let config = CountConfig::default();
        
        assert_eq!(config.memory_buffer_size, 10000);
        assert_eq!(config.flush_interval_seconds, 300);
        assert_eq!(config.data_dir, "./count_data");
        assert!(!config.s3_enabled);
        assert!(config.s3_bucket.is_none());
        assert!(config.s3_region.is_none());
        assert!(config.s3_prefix.is_none());
        assert_eq!(config.archival_age_days, 14);
        assert!(!config.cluster_enabled);
        assert!(config.node_id.is_none());
        assert!(config.bind_address.is_none());
        assert!(config.seed_nodes.is_empty());
        assert_eq!(config.replication_factor, 2);
    }

    #[test] 
    fn test_config_from_env_s3_settings() {
        // Save original env vars
        let original_s3_enabled = env::var("S3_ENABLED").ok();
        let original_s3_bucket = env::var("S3_BUCKET").ok();
        let original_s3_region = env::var("S3_REGION").ok();
        let original_s3_prefix = env::var("S3_PREFIX").ok();
        let original_archival_age = env::var("ARCHIVAL_AGE_DAYS").ok();

        // Set test env vars
        env::set_var("S3_ENABLED", "true");
        env::set_var("S3_BUCKET", "test-bucket");
        env::set_var("S3_REGION", "us-west-2");
        env::set_var("S3_PREFIX", "test-prefix");
        env::set_var("ARCHIVAL_AGE_DAYS", "7");

        let config = CountConfig::from_env();

        assert!(config.s3_enabled);
        assert_eq!(config.s3_bucket, Some("test-bucket".to_string()));
        assert_eq!(config.s3_region, Some("us-west-2".to_string()));
        assert_eq!(config.s3_prefix, Some("test-prefix".to_string()));
        assert_eq!(config.archival_age_days, 7);

        // Restore original env vars
        env::remove_var("S3_ENABLED");
        env::remove_var("S3_BUCKET");
        env::remove_var("S3_REGION");
        env::remove_var("S3_PREFIX");
        env::remove_var("ARCHIVAL_AGE_DAYS");

        if let Some(val) = original_s3_enabled { env::set_var("S3_ENABLED", val); }
        if let Some(val) = original_s3_bucket { env::set_var("S3_BUCKET", val); }
        if let Some(val) = original_s3_region { env::set_var("S3_REGION", val); }
        if let Some(val) = original_s3_prefix { env::set_var("S3_PREFIX", val); }
        if let Some(val) = original_archival_age { env::set_var("ARCHIVAL_AGE_DAYS", val); }
    }

    #[test]
    fn test_config_from_env_defaults() {
        // Save original env vars that might affect the test
        let original_replication_factor = env::var("REPLICATION_FACTOR").ok();
        let original_data_dir = env::var("DATA_DIR").ok();
        
        // Remove env vars to ensure defaults are used
        env::remove_var("REPLICATION_FACTOR");
        env::remove_var("DATA_DIR");
        
        // Test that defaults are used when env vars are not set
        let config = CountConfig::from_env();
        
        // These should fall back to defaults if env vars are not set
        assert_eq!(config.memory_buffer_size, 10000);
        assert_eq!(config.flush_interval_seconds, 300);
        assert_eq!(config.data_dir, "./count_data");
        assert_eq!(config.replication_factor, 2);
        assert_eq!(config.archival_age_days, 14);
        
        // Restore original env vars
        if let Some(val) = original_replication_factor { env::set_var("REPLICATION_FACTOR", val); }
        if let Some(val) = original_data_dir { env::set_var("DATA_DIR", val); }
    }

    #[test]
    fn test_config_from_env_cluster_settings() {
        // Save original env vars
        let original_cluster_enabled = env::var("CLUSTER_ENABLED").ok();
        let original_node_id = env::var("NODE_ID").ok();
        let original_bind_address = env::var("BIND_ADDRESS").ok();
        let original_cluster_nodes = env::var("CLUSTER_NODES").ok();
        let original_replication_factor = env::var("REPLICATION_FACTOR").ok();
        
        // Set test env vars
        env::set_var("CLUSTER_ENABLED", "true");
        env::set_var("NODE_ID", "42");
        env::set_var("BIND_ADDRESS", "0.0.0.0:8080");
        env::set_var("CLUSTER_NODES", "node1:8080,node2:8080,node3:8080");
        env::set_var("REPLICATION_FACTOR", "3");

        let config = CountConfig::from_env();

        assert!(config.cluster_enabled);
        assert_eq!(config.node_id, Some(42));
        assert_eq!(config.bind_address, Some("0.0.0.0:8080".to_string()));
        assert_eq!(config.seed_nodes, vec![
            "node1:8080".to_string(),
            "node2:8080".to_string(), 
            "node3:8080".to_string()
        ]);
        assert_eq!(config.replication_factor, 3);

        // Restore original env vars
        env::remove_var("CLUSTER_ENABLED");
        env::remove_var("NODE_ID");
        env::remove_var("BIND_ADDRESS");
        env::remove_var("CLUSTER_NODES");
        env::remove_var("REPLICATION_FACTOR");

        if let Some(val) = original_cluster_enabled { env::set_var("CLUSTER_ENABLED", val); }
        if let Some(val) = original_node_id { env::set_var("NODE_ID", val); }
        if let Some(val) = original_bind_address { env::set_var("BIND_ADDRESS", val); }
        if let Some(val) = original_cluster_nodes { env::set_var("CLUSTER_NODES", val); }
        if let Some(val) = original_replication_factor { env::set_var("REPLICATION_FACTOR", val); }
    }

    #[test]
    fn test_config_s3_enabled_without_bucket() {
        let mut config = CountConfig::default();
        config.s3_enabled = true;
        config.s3_bucket = None;
        
        // This configuration should be handled gracefully 
        // (S3 integration should be disabled with a warning in the actual implementation)
        assert!(config.s3_enabled);
        assert!(config.s3_bucket.is_none());
    }

    #[test]
    fn test_config_clone_and_debug() {
        let config = CountConfig {
            memory_buffer_size: 5000,
            flush_interval_seconds: 60,
            data_dir: "/tmp/test".to_string(),
            s3_enabled: true,
            s3_bucket: Some("test-bucket".to_string()),
            s3_region: Some("eu-west-1".to_string()),
            s3_prefix: Some("test-data".to_string()),
            archival_age_days: 30,
            cluster_enabled: true,
            node_id: Some(1),
            bind_address: Some("127.0.0.1:9090".to_string()),
            seed_nodes: vec!["peer1:9090".to_string()],
            replication_factor: 1,
        };

        // Test Clone trait
        let cloned_config = config.clone();
        assert_eq!(cloned_config.memory_buffer_size, config.memory_buffer_size);
        assert_eq!(cloned_config.s3_bucket, config.s3_bucket);
        assert_eq!(cloned_config.archival_age_days, config.archival_age_days);

        // Test Debug trait (should not panic)
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("CountConfig"));
        assert!(debug_str.contains("test-bucket"));
    }

    #[test]
    fn test_series_key_from_string() {
        let key1 = SeriesKey::from("cpu.usage");
        let key2 = SeriesKey::from("memory.usage".to_string());
        
        assert_eq!(key1.0, "cpu.usage");
        assert_eq!(key2.0, "memory.usage");
    }

    #[test]
    fn test_data_point_creation() {
        let point = DataPoint::new(1234567890, 42.5);
        
        assert_eq!(point.timestamp, 1234567890);
        assert_eq!(point.value, 42.5);
    }

    #[tokio::test]
    async fn test_count_db_basic_operations() {
        use tempfile::TempDir;
        
        let temp_dir = TempDir::new().unwrap();
        let config = CountConfig {
            memory_buffer_size: 100,
            flush_interval_seconds: 1,
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            s3_enabled: false,
            s3_bucket: None,
            s3_region: None,
            s3_prefix: None,
            archival_age_days: 14,
            cluster_enabled: false,
            node_id: None,
            bind_address: None,
            seed_nodes: Vec::new(),
            replication_factor: 2,
        };

        let db = CountDB::new(config).await.unwrap();
        let series = SeriesKey::from("test.metric");
        let point = DataPoint::new(1000, 50.0);

        // Test insert
        db.insert(series.clone(), point).await.unwrap();

        // Test query
        let results = db.query_range(series.clone(), 500, 1500).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].timestamp, 1000);
        assert_eq!(results[0].value, 50.0);

        // Test aggregated query
        let avg = db.query_aggregated(
            series,
            500,
            1500,
            crate::query::AggregationType::Mean
        ).await.unwrap();
        assert_eq!(avg, 50.0);

        // Test shutdown
        db.shutdown().await.unwrap();
    }
}