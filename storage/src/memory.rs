use tsdb_core::{TimeSeriesKey, DataPoint, TimeSeries, CompressedBlock};
use crate::block::TimeSeriesBlock;
use crate::error::StorageError;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::BTreeMap;

pub struct TSMap {
    series: DashMap<TimeSeriesKey, Arc<RwLock<TimeSeriesStorage>>>,
}

struct TimeSeriesStorage {
    key: TimeSeriesKey,
    sealed_blocks: Vec<CompressedBlock>,
    current_block: Option<TimeSeriesBlock>,
}

impl TSMap {
    pub fn new() -> Self {
        Self {
            series: DashMap::new(),
        }
    }
    
    pub fn insert(&self, key: TimeSeriesKey, point: DataPoint) -> Result<(), StorageError> {
        let storage = self.series
            .entry(key.clone())
            .or_insert_with(|| Arc::new(RwLock::new(TimeSeriesStorage::new(key))));
            
        let mut storage = storage.write();
        storage.insert_point(point)
    }
    
    pub fn get_series(&self, key: &TimeSeriesKey) -> Option<TimeSeries> {
        let storage = self.series.get(key)?;
        let storage = storage.read();
        Some(storage.to_time_series())
    }
    
    pub fn scan_range(&self, key: &TimeSeriesKey, start: u64, end: u64) -> Result<Vec<DataPoint>, StorageError> {
        if start > end {
            return Err(StorageError::InvalidTimeRange(start, end));
        }
        
        let storage = self.series.get(key)
            .ok_or_else(|| StorageError::KeyNotFound(key.clone()))?;
        let storage = storage.read();
        
        storage.scan_range(start, end)
    }
    
    pub fn keys(&self) -> Vec<TimeSeriesKey> {
        self.series.iter().map(|entry| entry.key().clone()).collect()
    }
    
    pub fn len(&self) -> usize {
        self.series.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }
    
    pub fn seal_expired_blocks(&self) {
        for entry in self.series.iter() {
            let mut storage = entry.write();
            storage.seal_if_expired();
        }
    }
    
    pub fn get_stats(&self) -> TSMapStats {
        let mut total_points = 0;
        let mut total_blocks = 0;
        let mut total_compressed_size = 0;
        
        for entry in self.series.iter() {
            let storage = entry.read();
            let stats = storage.get_stats();
            total_points += stats.point_count;
            total_blocks += stats.block_count;
            total_compressed_size += stats.compressed_size;
        }
        
        TSMapStats {
            series_count: self.len(),
            total_points,
            total_blocks,
            total_compressed_size,
        }
    }
}

impl Default for TSMap {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TSMapStats {
    pub series_count: usize,
    pub total_points: usize,
    pub total_blocks: usize,
    pub total_compressed_size: usize,
}

#[derive(Debug, Clone)]
pub struct TimeSeriesStats {
    pub point_count: usize,
    pub block_count: usize,
    pub compressed_size: usize,
}

impl TimeSeriesStorage {
    fn new(key: TimeSeriesKey) -> Self {
        Self {
            key,
            sealed_blocks: Vec::new(),
            current_block: None,
        }
    }
    
    fn insert_point(&mut self, point: DataPoint) -> Result<(), StorageError> {
        if let Some(ref mut block) = self.current_block {
            if block.can_accept(point.timestamp) {
                return block.add_point(point);
            } else {
                self.seal_current_block()?;
            }
        }
        
        let mut new_block = TimeSeriesBlock::new(point.timestamp);
        new_block.add_point(point)?;
        self.current_block = Some(new_block);
        
        Ok(())
    }
    
    fn seal_current_block(&mut self) -> Result<(), StorageError> {
        if let Some(mut block) = self.current_block.take() {
            block.seal();
            let compressed = block.compress()?;
            if compressed.count > 0 {
                self.sealed_blocks.push(compressed);
            }
        }
        Ok(())
    }
    
    fn seal_if_expired(&mut self) {
        if let Some(ref block) = self.current_block {
            if block.should_seal() {
                let _ = self.seal_current_block();
            }
        }
    }
    
    fn to_time_series(&self) -> TimeSeries {
        let current_points = self.current_block
            .as_ref()
            .map(|block| block.points.clone())
            .unwrap_or_default();
            
        TimeSeries {
            key: self.key.clone(),
            blocks: self.sealed_blocks.clone(),
            current_block: if current_points.is_empty() { None } else { Some(current_points) },
        }
    }
    
    fn scan_range(&self, start: u64, end: u64) -> Result<Vec<DataPoint>, StorageError> {
        let mut points = Vec::new();
        
        for block in &self.sealed_blocks {
            if block.end_timestamp < start || block.start_timestamp > end {
                continue;
            }
            
            // For now, we can't decompress blocks in this simple implementation
            // In a real implementation, we'd decompress the block and filter points
        }
        
        if let Some(ref block) = self.current_block {
            for point in &block.points {
                if point.timestamp >= start && point.timestamp <= end {
                    points.push(point.clone());
                }
            }
        }
        
        points.sort_by_key(|p| p.timestamp);
        Ok(points)
    }
    
    fn get_stats(&self) -> TimeSeriesStats {
        let current_points = self.current_block
            .as_ref()
            .map(|block| block.points.len())
            .unwrap_or(0);
            
        let sealed_points: usize = self.sealed_blocks.iter().map(|b| b.count).sum();
        let compressed_size: usize = self.sealed_blocks.iter()
            .map(|b| b.compressed_data.len())
            .sum();
            
        TimeSeriesStats {
            point_count: current_points + sealed_points,
            block_count: self.sealed_blocks.len() + if self.current_block.is_some() { 1 } else { 0 },
            compressed_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsmap_creation() {
        let tsmap = TSMap::new();
        assert!(tsmap.is_empty());
        assert_eq!(tsmap.len(), 0);
    }

    #[test]
    fn test_tsmap_insert_single_point() {
        let tsmap = TSMap::new();
        let point = DataPoint::new(1000, 42.5);
        
        assert!(tsmap.insert("test.metric".to_string(), point).is_ok());
        assert_eq!(tsmap.len(), 1);
        assert!(!tsmap.is_empty());
    }

    #[test]
    fn test_tsmap_insert_multiple_points() {
        let tsmap = TSMap::new();
        let key = "test.metric".to_string();
        
        for i in 0..5 {
            let point = DataPoint::new(1000 + i * 100, i as f64);
            assert!(tsmap.insert(key.clone(), point).is_ok());
        }
        
        assert_eq!(tsmap.len(), 1);
        
        let series = tsmap.get_series(&key).unwrap();
        assert_eq!(series.point_count(), 5);
    }

    #[test]
    fn test_tsmap_multiple_series() {
        let tsmap = TSMap::new();
        
        for i in 0..3 {
            let key = format!("test.metric.{}", i);
            let point = DataPoint::new(1000, i as f64);
            assert!(tsmap.insert(key, point).is_ok());
        }
        
        assert_eq!(tsmap.len(), 3);
    }

    #[test]
    fn test_tsmap_get_series() {
        let tsmap = TSMap::new();
        let key = "test.metric".to_string();
        let point = DataPoint::new(1000, 42.5);
        
        tsmap.insert(key.clone(), point.clone()).unwrap();
        
        let series = tsmap.get_series(&key).unwrap();
        assert_eq!(series.key, key);
        assert_eq!(series.point_count(), 1);
    }

    #[test]
    fn test_tsmap_get_nonexistent_series() {
        let tsmap = TSMap::new();
        assert!(tsmap.get_series(&"nonexistent".to_string()).is_none());
    }

    #[test]
    fn test_tsmap_scan_range() {
        let tsmap = TSMap::new();
        let key = "test.metric".to_string();
        
        for i in 0..10 {
            let point = DataPoint::new(1000 + i * 100, i as f64);
            tsmap.insert(key.clone(), point).unwrap();
        }
        
        let points = tsmap.scan_range(&key, 1200, 1600).unwrap();
        assert_eq!(points.len(), 5); // timestamps 1200, 1300, 1400, 1500, 1600
        assert_eq!(points[0].timestamp, 1200);
        assert_eq!(points[4].timestamp, 1600);
    }

    #[test]
    fn test_tsmap_scan_range_invalid() {
        let tsmap = TSMap::new();
        let key = "test.metric".to_string();
        
        assert!(tsmap.scan_range(&key, 1000, 500).is_err());
    }

    #[test]
    fn test_tsmap_scan_range_nonexistent_key() {
        let tsmap = TSMap::new();
        assert!(tsmap.scan_range(&"nonexistent".to_string(), 0, 1000).is_err());
    }

    #[test]
    fn test_tsmap_keys() {
        let tsmap = TSMap::new();
        
        let keys = vec!["metric.1".to_string(), "metric.2".to_string(), "metric.3".to_string()];
        for key in &keys {
            let point = DataPoint::new(1000, 42.5);
            tsmap.insert(key.clone(), point).unwrap();
        }
        
        let mut result_keys = tsmap.keys();
        result_keys.sort();
        
        let mut expected_keys = keys;
        expected_keys.sort();
        
        assert_eq!(result_keys, expected_keys);
    }

    #[test]
    fn test_tsmap_stats() {
        let tsmap = TSMap::new();
        let key = "test.metric".to_string();
        
        for i in 0..5 {
            let point = DataPoint::new(1000 + i * 100, i as f64);
            tsmap.insert(key.clone(), point).unwrap();
        }
        
        let stats = tsmap.get_stats();
        assert_eq!(stats.series_count, 1);
        assert_eq!(stats.total_points, 5);
    }
}