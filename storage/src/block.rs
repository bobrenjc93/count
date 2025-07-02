use tsdb_core::{DataPoint, CompressedBlock};
use compression::{TimestampCompressor, ValueCompressor, BitWriter};
use crate::error::StorageError;
use std::time::{SystemTime, UNIX_EPOCH};

const BLOCK_DURATION_MS: u64 = 2 * 60 * 60 * 1000; // 2 hours in milliseconds

#[derive(Debug, Clone)]
pub struct TimeSeriesBlock {
    pub start_time: u64,
    pub end_time: u64,
    pub points: Vec<DataPoint>,
    pub is_sealed: bool,
}

impl TimeSeriesBlock {
    pub fn new(start_time: u64) -> Self {
        Self {
            start_time,
            end_time: start_time + BLOCK_DURATION_MS,
            points: Vec::new(),
            is_sealed: false,
        }
    }
    
    pub fn can_accept(&self, timestamp: u64) -> bool {
        !self.is_sealed && timestamp >= self.start_time && timestamp < self.end_time
    }
    
    pub fn add_point(&mut self, point: DataPoint) -> Result<(), StorageError> {
        if self.is_sealed {
            return Err(StorageError::BlockSealed);
        }
        
        if !self.can_accept(point.timestamp) {
            return Err(StorageError::InvalidTimeRange(point.timestamp, self.end_time));
        }
        
        self.points.push(point);
        Ok(())
    }
    
    pub fn seal(&mut self) {
        self.is_sealed = true;
    }
    
    pub fn compress(&self) -> Result<CompressedBlock, StorageError> {
        if self.points.is_empty() {
            return Ok(CompressedBlock {
                start_timestamp: self.start_time,
                end_timestamp: self.start_time,
                count: 0,
                compressed_data: Vec::new(),
            });
        }
        
        let mut writer = BitWriter::new();
        let first_point = &self.points[0];
        
        let mut timestamp_compressor = TimestampCompressor::new(first_point.timestamp);
        let mut value_compressor = ValueCompressor::new(first_point.value);
        
        for (i, point) in self.points.iter().enumerate() {
            if i > 0 {
                timestamp_compressor.compress(point.timestamp, &mut writer)?;
                value_compressor.compress(point.value, &mut writer)?;
            }
        }
        
        let compressed_data = writer.finish();
        
        Ok(CompressedBlock {
            start_timestamp: first_point.timestamp,
            end_timestamp: self.points.last().unwrap().timestamp,
            count: self.points.len(),
            compressed_data,
        })
    }
    
    pub fn should_seal(&self) -> bool {
        if self.is_sealed {
            return false;
        }
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
            
        now >= self.end_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_creation() {
        let block = TimeSeriesBlock::new(1000);
        assert_eq!(block.start_time, 1000);
        assert_eq!(block.end_time, 1000 + BLOCK_DURATION_MS);
        assert!(!block.is_sealed);
        assert!(block.points.is_empty());
    }

    #[test]
    fn test_block_can_accept() {
        let block = TimeSeriesBlock::new(1000);
        
        assert!(block.can_accept(1000));
        assert!(block.can_accept(1500));
        assert!(block.can_accept(1000 + BLOCK_DURATION_MS - 1));
        assert!(!block.can_accept(999));
        assert!(!block.can_accept(1000 + BLOCK_DURATION_MS));
    }

    #[test]
    fn test_block_add_point() {
        let mut block = TimeSeriesBlock::new(1000);
        let point = DataPoint::new(1500, 42.5);
        
        assert!(block.add_point(point.clone()).is_ok());
        assert_eq!(block.points.len(), 1);
        assert_eq!(block.points[0], point);
    }

    #[test]
    fn test_block_add_point_out_of_range() {
        let mut block = TimeSeriesBlock::new(1000);
        let point = DataPoint::new(999, 42.5);
        
        assert!(block.add_point(point).is_err());
        assert!(block.points.is_empty());
    }

    #[test]
    fn test_block_add_point_sealed() {
        let mut block = TimeSeriesBlock::new(1000);
        block.seal();
        let point = DataPoint::new(1500, 42.5);
        
        assert!(block.add_point(point).is_err());
        assert!(block.points.is_empty());
    }

    #[test]
    fn test_block_seal() {
        let mut block = TimeSeriesBlock::new(1000);
        assert!(!block.is_sealed);
        
        block.seal();
        assert!(block.is_sealed);
    }

    #[test]
    fn test_block_compress_empty() {
        let block = TimeSeriesBlock::new(1000);
        let compressed = block.compress().unwrap();
        
        assert_eq!(compressed.count, 0);
        assert_eq!(compressed.start_timestamp, 1000);
        assert_eq!(compressed.end_timestamp, 1000);
        assert!(compressed.compressed_data.is_empty());
    }

    #[test]
    fn test_block_compress_single_point() {
        let mut block = TimeSeriesBlock::new(1000);
        block.add_point(DataPoint::new(1500, 42.5)).unwrap();
        
        let compressed = block.compress().unwrap();
        assert_eq!(compressed.count, 1);
        assert_eq!(compressed.start_timestamp, 1500);
        assert_eq!(compressed.end_timestamp, 1500);
    }

    #[test]
    fn test_block_compress_multiple_points() {
        let mut block = TimeSeriesBlock::new(1000);
        block.add_point(DataPoint::new(1500, 42.5)).unwrap();
        block.add_point(DataPoint::new(1600, 43.0)).unwrap();
        block.add_point(DataPoint::new(1700, 43.5)).unwrap();
        
        let compressed = block.compress().unwrap();
        assert_eq!(compressed.count, 3);
        assert_eq!(compressed.start_timestamp, 1500);
        assert_eq!(compressed.end_timestamp, 1700);
        assert!(!compressed.compressed_data.is_empty());
    }
}