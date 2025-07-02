use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type TimeSeriesKey = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataPoint {
    pub timestamp: u64,
    pub value: f64,
}

impl DataPoint {
    pub fn new(timestamp: u64, value: f64) -> Self {
        Self { timestamp, value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedBlock {
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub count: usize,
    pub compressed_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeries {
    pub key: TimeSeriesKey,
    pub blocks: Vec<CompressedBlock>,
    pub current_block: Option<Vec<DataPoint>>,
}

impl TimeSeries {
    pub fn new(key: TimeSeriesKey) -> Self {
        Self {
            key,
            blocks: Vec::new(),
            current_block: Some(Vec::new()),
        }
    }
    
    pub fn add_point(&mut self, point: DataPoint) {
        if let Some(ref mut current) = self.current_block {
            current.push(point);
        }
    }
    
    pub fn seal_current_block(&mut self) -> Option<Vec<DataPoint>> {
        self.current_block.take().map(|points| {
            if !points.is_empty() {
                self.current_block = Some(Vec::new());
            }
            points
        })
    }
    
    pub fn point_count(&self) -> usize {
        let block_points: usize = self.blocks.iter().map(|b| b.count).sum();
        let current_points = self.current_block.as_ref()
            .map(|b| b.len())
            .unwrap_or(0);
        block_points + current_points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datapoint_creation() {
        let point = DataPoint::new(1609459200000, 42.5);
        assert_eq!(point.timestamp, 1609459200000);
        assert_eq!(point.value, 42.5);
    }

    #[test]
    fn test_datapoint_equality() {
        let point1 = DataPoint::new(1609459200000, 42.5);
        let point2 = DataPoint::new(1609459200000, 42.5);
        let point3 = DataPoint::new(1609459200001, 42.5);
        
        assert_eq!(point1, point2);
        assert_ne!(point1, point3);
    }

    #[test]
    fn test_timeseries_creation() {
        let ts = TimeSeries::new("test.metric".to_string());
        assert_eq!(ts.key, "test.metric");
        assert!(ts.blocks.is_empty());
        assert!(ts.current_block.is_some());
        assert_eq!(ts.point_count(), 0);
    }

    #[test]
    fn test_timeseries_add_point() {
        let mut ts = TimeSeries::new("test.metric".to_string());
        let point = DataPoint::new(1609459200000, 42.5);
        
        ts.add_point(point.clone());
        assert_eq!(ts.point_count(), 1);
        
        if let Some(ref block) = ts.current_block {
            assert_eq!(block.len(), 1);
            assert_eq!(block[0], point);
        }
    }

    #[test]
    fn test_timeseries_multiple_points() {
        let mut ts = TimeSeries::new("test.metric".to_string());
        
        for i in 0..5 {
            let point = DataPoint::new(1609459200000 + i * 1000, i as f64);
            ts.add_point(point);
        }
        
        assert_eq!(ts.point_count(), 5);
    }

    #[test]
    fn test_timeseries_seal_block() {
        let mut ts = TimeSeries::new("test.metric".to_string());
        
        ts.add_point(DataPoint::new(1609459200000, 1.0));
        ts.add_point(DataPoint::new(1609459201000, 2.0));
        
        let sealed = ts.seal_current_block();
        assert!(sealed.is_some());
        assert_eq!(sealed.unwrap().len(), 2);
        assert_eq!(ts.point_count(), 0);
        assert!(ts.current_block.is_some());
    }

    #[test]
    fn test_timeseries_seal_empty_block() {
        let mut ts = TimeSeries::new("test.metric".to_string());
        let sealed = ts.seal_current_block();
        assert!(sealed.is_some());
        assert_eq!(sealed.unwrap().len(), 0);
    }
}