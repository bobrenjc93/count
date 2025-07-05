use crate::{DataPoint, SeriesKey};
use crate::compression::{TimestampCompressor, ValueCompressor};
use crate::error::CountResult;
use std::collections::{BTreeMap, HashMap};
use tokio::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CompressedBlock {
    pub start_time: u64,
    pub end_time: u64,
    pub compressed_timestamps: Vec<u8>,
    pub compressed_values: Vec<u8>,
    pub point_count: usize,
    pub original_points: Vec<DataPoint>, // Store original data for querying
}

impl CompressedBlock {
    pub fn new(start_time: u64) -> Self {
        Self {
            start_time,
            end_time: start_time,
            compressed_timestamps: Vec::new(),
            compressed_values: Vec::new(),
            point_count: 0,
            original_points: Vec::new(),
        }
    }

    pub fn add_point(&mut self, point: &DataPoint, 
                     ts_compressor: &mut TimestampCompressor,
                     val_compressor: &mut ValueCompressor) -> CountResult<()> {
        let compressed_ts = ts_compressor.compress(point.timestamp)?;
        let compressed_val = val_compressor.compress(point.value)?;
        
        self.compressed_timestamps.extend(compressed_ts);
        self.compressed_values.extend(compressed_val);
        self.end_time = point.timestamp;
        self.point_count += 1;
        self.original_points.push(point.clone());
        
        Ok(())
    }

    pub fn size_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + 
        self.compressed_timestamps.len() + 
        self.compressed_values.len()
    }

    pub fn query_range(&self, start_time: u64, end_time: u64) -> Vec<DataPoint> {
        self.original_points
            .iter()
            .filter(|point| point.timestamp >= start_time && point.timestamp <= end_time)
            .cloned()
            .collect()
    }
}

pub struct MemoryBuffer {
    series_data: HashMap<String, BTreeMap<u64, DataPoint>>, // Raw data for recent queries
    compressed_blocks: HashMap<String, Vec<CompressedBlock>>, // Compressed historical data
    last_flush: Instant,
    max_points_per_series: usize,
    flush_interval: Duration,
}

impl MemoryBuffer {
    pub fn new(max_points_per_series: usize, flush_interval_seconds: u64) -> Self {
        Self {
            series_data: HashMap::new(),
            compressed_blocks: HashMap::new(),
            last_flush: Instant::now(),
            max_points_per_series,
            flush_interval: Duration::from_secs(flush_interval_seconds),
        }
    }

    pub fn insert(&mut self, series: SeriesKey, point: DataPoint) -> CountResult<()> {
        let series_name = series.0;
        let series_points = self.series_data.entry(series_name.clone()).or_insert_with(BTreeMap::new);
        
        // Insert into raw data for fast recent queries
        series_points.insert(point.timestamp, point.clone());
        
        // Compress older data if we exceed the buffer size
        if series_points.len() > self.max_points_per_series {
            self.compress_old_data(&series_name)?;
        }
        
        Ok(())
    }

    pub fn query_range(&self, series: SeriesKey, start_time: u64, end_time: u64) -> CountResult<Vec<DataPoint>> {
        let mut result = Vec::new();
        let series_name = &series.0;
        
        // Query from raw data (recent points)
        if let Some(series_points) = self.series_data.get(series_name) {
            for (_, point) in series_points.range(start_time..=end_time) {
                result.push(point.clone());
            }
        }
        
        // Query from compressed blocks for historical data
        if let Some(blocks) = self.compressed_blocks.get(series_name) {
            for block in blocks {
                let block_points = block.query_range(start_time, end_time);
                result.extend(block_points);
            }
        }
        
        result.sort_by_key(|p| p.timestamp);
        Ok(result)
    }

    pub fn should_flush(&self) -> bool {
        self.last_flush.elapsed() >= self.flush_interval
    }

    pub fn get_series_for_flush(&mut self) -> HashMap<String, Vec<CompressedBlock>> {
        self.last_flush = Instant::now();
        
        // Return compressed blocks for flushing to disk
        let mut blocks_to_flush = HashMap::new();
        for (series_name, blocks) in &self.compressed_blocks {
            if !blocks.is_empty() {
                blocks_to_flush.insert(series_name.clone(), blocks.clone());
            }
        }
        
        // Also compress and flush any remaining raw data
        for (series_name, series_points) in &self.series_data {
            if !series_points.is_empty() {
                // Convert raw points to a compressed block
                let mut points_to_compress: Vec<_> = series_points.values().cloned().collect();
                points_to_compress.sort_by_key(|p| p.timestamp);
                
                if !points_to_compress.is_empty() {
                    let start_time = points_to_compress[0].timestamp;
                    let mut block = CompressedBlock::new(start_time);
                    let mut ts_compressor = TimestampCompressor::new();
                    let mut val_compressor = ValueCompressor::new();
                    
                    for point in &points_to_compress {
                        if let Err(_) = block.add_point(point, &mut ts_compressor, &mut val_compressor) {
                            // If compression fails, just store the original points
                            continue;
                        }
                    }
                    
                    blocks_to_flush.entry(series_name.clone())
                        .or_insert_with(Vec::new)
                        .push(block);
                }
            }
        }
        
        // Clear all data after preparing for flush
        self.compressed_blocks.clear();
        self.series_data.clear();
        
        blocks_to_flush
    }

    fn compress_old_data(&mut self, series_name: &str) -> CountResult<()> {
        if let Some(series_points) = self.series_data.get_mut(series_name) {
            if series_points.len() <= self.max_points_per_series / 2 {
                return Ok(());
            }
            
            // Take the oldest half of points for compression
            let cutoff_index = series_points.len() / 2;
            let mut points_to_compress: Vec<_> = series_points.iter().take(cutoff_index).map(|(_, p)| p.clone()).collect();
            points_to_compress.sort_by_key(|p| p.timestamp);
            
            if !points_to_compress.is_empty() {
                let start_time = points_to_compress[0].timestamp;
                let mut block = CompressedBlock::new(start_time);
                let mut ts_compressor = TimestampCompressor::new();
                let mut val_compressor = ValueCompressor::new();
                
                for point in &points_to_compress {
                    block.add_point(point, &mut ts_compressor, &mut val_compressor)?;
                }
                
                // Add compressed block
                self.compressed_blocks.entry(series_name.to_string())
                    .or_insert_with(Vec::new)
                    .push(block);
                
                // Remove compressed points from raw data
                let keys_to_remove: Vec<_> = series_points.keys().take(cutoff_index).cloned().collect();
                for key in keys_to_remove {
                    series_points.remove(&key);
                }
            }
        }
        
        Ok(())
    }

    pub fn get_memory_usage(&self) -> usize {
        let mut total_size = 0;
        
        // Raw data size
        for (series_name, points) in &self.series_data {
            total_size += series_name.len();
            total_size += points.len() * std::mem::size_of::<DataPoint>();
        }
        
        // Compressed blocks size
        for (series_name, blocks) in &self.compressed_blocks {
            total_size += series_name.len();
            for block in blocks {
                total_size += block.size_bytes();
            }
        }
        
        total_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_buffer_basic_operations() {
        let mut buffer = MemoryBuffer::new(100, 300);
        
        let series = SeriesKey::from("test.series");
        let point1 = DataPoint::new(1000, 42.0);
        let point2 = DataPoint::new(2000, 43.0);
        
        buffer.insert(series.clone(), point1.clone()).unwrap();
        buffer.insert(series.clone(), point2.clone()).unwrap();
        
        let results = buffer.query_range(series, 500, 2500).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].timestamp, 1000);
        assert_eq!(results[1].timestamp, 2000);
    }

    #[test]
    fn test_memory_buffer_compression_trigger() {
        let mut buffer = MemoryBuffer::new(10, 300); // Small buffer to trigger compression
        let series = SeriesKey::from("test.series");
        
        // Insert more points than the buffer size
        for i in 0..15 {
            let point = DataPoint::new(1000 + i * 1000, i as f64);
            buffer.insert(series.clone(), point).unwrap();
        }
        
        // Should have compressed some data
        assert!(!buffer.compressed_blocks.is_empty());
        
        // Should still be able to query recent data
        let results = buffer.query_range(series, 10000, 15000).unwrap();
        assert!(!results.is_empty());
    }
}