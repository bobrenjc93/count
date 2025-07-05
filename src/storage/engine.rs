use crate::{CountConfig, DataPoint, SeriesKey};
use crate::error::CountResult;
use crate::query::AggregationType;
use crate::storage::{disk::DiskStorage, memory::MemoryBuffer};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

pub struct StorageEngine {
    memory_buffer: Arc<Mutex<MemoryBuffer>>,
    disk_storage: Arc<DiskStorage>,
    _flush_handle: tokio::task::JoinHandle<()>,
}

impl StorageEngine {
    pub async fn new(config: &CountConfig) -> CountResult<Self> {
        let memory_buffer = Arc::new(Mutex::new(MemoryBuffer::new(
            config.memory_buffer_size,
            config.flush_interval_seconds,
        )));
        
        let disk_storage = Arc::new(DiskStorage::new(&config.data_dir).await?);
        
        // Start background flush task
        let flush_handle = {
            let memory_buffer = Arc::clone(&memory_buffer);
            let disk_storage = Arc::clone(&disk_storage);
            let flush_interval = Duration::from_secs(config.flush_interval_seconds);
            
            tokio::spawn(async move {
                let mut interval_timer = interval(flush_interval);
                
                loop {
                    interval_timer.tick().await;
                    
                    let blocks_to_flush = {
                        let mut buffer = memory_buffer.lock().await;
                        if buffer.should_flush() {
                            buffer.get_series_for_flush()
                        } else {
                            continue;
                        }
                    };
                    
                    if !blocks_to_flush.is_empty() {
                        if let Err(e) = disk_storage.flush_blocks(blocks_to_flush).await {
                            eprintln!("Error flushing to disk: {}", e);
                        }
                    }
                }
            })
        };
        
        Ok(Self {
            memory_buffer,
            disk_storage,
            _flush_handle: flush_handle,
        })
    }

    pub async fn insert(&mut self, series: SeriesKey, point: DataPoint) -> CountResult<()> {
        let mut buffer = self.memory_buffer.lock().await;
        buffer.insert(series, point)
    }

    pub async fn query_range(&self, series: SeriesKey, start_time: u64, end_time: u64) -> CountResult<Vec<DataPoint>> {
        let mut result = Vec::new();
        
        // Query from memory buffer first
        {
            let buffer = self.memory_buffer.lock().await;
            let memory_points = buffer.query_range(series.clone(), start_time, end_time)?;
            result.extend(memory_points);
        }
        
        // Query from disk storage for historical data
        let disk_blocks = self.disk_storage.load_blocks_for_range(&series.0, start_time, end_time).await.unwrap_or_else(|_| Vec::new());
        
        // Decompress disk blocks and extract points in range
        for block in disk_blocks {
            let block_points = block.query_range(start_time, end_time);
            result.extend(block_points);
        }
        
        // Sort by timestamp and deduplicate if necessary
        result.sort_by_key(|p| p.timestamp);
        result.dedup_by_key(|p| p.timestamp);
        
        Ok(result)
    }

    pub async fn query_aggregated(
        &self,
        series: SeriesKey,
        start_time: u64,
        end_time: u64,
        aggregation: AggregationType,
    ) -> CountResult<f64> {
        let points = self.query_range(series, start_time, end_time).await?;
        
        if points.is_empty() {
            return Ok(0.0);
        }
        
        match aggregation {
            AggregationType::Mean => {
                let sum: f64 = points.iter().map(|p| p.value).sum();
                Ok(sum / points.len() as f64)
            }
            AggregationType::Sum => {
                Ok(points.iter().map(|p| p.value).sum())
            }
            AggregationType::Min => {
                Ok(points.iter().map(|p| p.value).fold(f64::INFINITY, f64::min))
            }
            AggregationType::Max => {
                Ok(points.iter().map(|p| p.value).fold(f64::NEG_INFINITY, f64::max))
            }
            AggregationType::Count => {
                Ok(points.len() as f64)
            }
        }
    }

    pub async fn get_series_list(&self) -> CountResult<Vec<String>> {
        // Combine series from memory and disk
        let mut series_set = std::collections::HashSet::new();
        
        // Add series from disk
        let disk_series = self.disk_storage.get_series_list().await?;
        series_set.extend(disk_series);
        
        // Add series from memory
        {
            let _buffer = self.memory_buffer.lock().await;
            // Note: This would require exposing series names from MemoryBuffer
            // For now, we'll just return disk series
        }
        
        Ok(series_set.into_iter().collect())
    }

    pub async fn cleanup_old_data(&self, cutoff_time: u64) -> CountResult<usize> {
        self.disk_storage.cleanup_old_data(cutoff_time).await
    }

    pub async fn get_memory_usage(&self) -> usize {
        let buffer = self.memory_buffer.lock().await;
        buffer.get_memory_usage()
    }

    pub async fn force_flush(&self) -> CountResult<()> {
        let blocks_to_flush = {
            let mut buffer = self.memory_buffer.lock().await;
            buffer.get_series_for_flush()
        };
        
        if !blocks_to_flush.is_empty() {
            self.disk_storage.flush_blocks(blocks_to_flush).await?;
        }
        
        Ok(())
    }

    pub async fn shutdown(&mut self) -> CountResult<()> {
        // Force a final flush before shutdown
        self.force_flush().await?;
        
        // The flush handle will be automatically cancelled when the StorageEngine is dropped
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_engine_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = CountConfig {
            memory_buffer_size: 100,
            flush_interval_seconds: 1,
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            cluster_enabled: false,
            node_id: None,
            bind_address: None,
            seed_nodes: Vec::new(),
            replication_factor: 2,
        };
        
        let mut engine = StorageEngine::new(&config).await.unwrap();
        
        let series = SeriesKey::from("test.cpu");
        let point1 = DataPoint::new(1000, 50.0);
        let point2 = DataPoint::new(2000, 60.0);
        
        engine.insert(series.clone(), point1).await.unwrap();
        engine.insert(series.clone(), point2).await.unwrap();
        
        let results = engine.query_range(series.clone(), 500, 2500).await.unwrap();
        assert_eq!(results.len(), 2);
        
        let avg = engine.query_aggregated(series, 500, 2500, AggregationType::Mean).await.unwrap();
        assert_eq!(avg, 55.0);
    }

    #[tokio::test]
    async fn test_storage_engine_aggregations() {
        let temp_dir = TempDir::new().unwrap();
        let config = CountConfig {
            memory_buffer_size: 100,
            flush_interval_seconds: 1,
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            cluster_enabled: false,
            node_id: None,
            bind_address: None,
            seed_nodes: Vec::new(),
            replication_factor: 2,
        };
        
        let mut engine = StorageEngine::new(&config).await.unwrap();
        let series = SeriesKey::from("test.metrics");
        
        // Insert test data
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        for (i, &value) in values.iter().enumerate() {
            let point = DataPoint::new(1000 + i as u64 * 1000, value);
            engine.insert(series.clone(), point).await.unwrap();
        }
        
        // Test different aggregations
        let sum = engine.query_aggregated(series.clone(), 0, 10000, AggregationType::Sum).await.unwrap();
        assert_eq!(sum, 150.0);
        
        let mean = engine.query_aggregated(series.clone(), 0, 10000, AggregationType::Mean).await.unwrap();
        assert_eq!(mean, 30.0);
        
        let min = engine.query_aggregated(series.clone(), 0, 10000, AggregationType::Min).await.unwrap();
        assert_eq!(min, 10.0);
        
        let max = engine.query_aggregated(series.clone(), 0, 10000, AggregationType::Max).await.unwrap();
        assert_eq!(max, 50.0);
        
        let count = engine.query_aggregated(series, 0, 10000, AggregationType::Count).await.unwrap();
        assert_eq!(count, 5.0);
    }
}