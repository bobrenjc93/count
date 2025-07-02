use tsdb_core::{TimeSeriesKey, DataPoint};
use storage::TSMap;
use crate::error::QueryError;
use crate::aggregation::{Aggregation, AggregatedPoint, aggregate_points, downsample_points};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Query {
    pub key: TimeSeriesKey,
    pub start_time: u64,
    pub end_time: u64,
    pub aggregation: Option<Aggregation>,
    pub window_size: Option<u64>,
    pub max_points: Option<usize>,
}

impl Query {
    pub fn new(key: TimeSeriesKey, start_time: u64, end_time: u64) -> Self {
        Self {
            key,
            start_time,
            end_time,
            aggregation: None,
            window_size: None,
            max_points: None,
        }
    }
    
    pub fn with_aggregation(mut self, aggregation: Aggregation, window_size: u64) -> Self {
        self.aggregation = Some(aggregation);
        self.window_size = Some(window_size);
        self
    }
    
    pub fn with_max_points(mut self, max_points: usize) -> Self {
        self.max_points = Some(max_points);
        self
    }
    
    pub fn validate(&self) -> Result<(), QueryError> {
        if self.start_time > self.end_time {
            return Err(QueryError::InvalidTimeRange(self.start_time, self.end_time));
        }
        
        if self.aggregation.is_some() && self.window_size.is_none() {
            return Err(QueryError::InvalidQuery(
                "Aggregation requires window_size".to_string()
            ));
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum QueryResult {
    Points(Vec<DataPoint>),
    Aggregated(Vec<AggregatedPoint>),
}

impl QueryResult {
    pub fn len(&self) -> usize {
        match self {
            QueryResult::Points(points) => points.len(),
            QueryResult::Aggregated(points) => points.len(),
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct QueryEngine {
    storage: Arc<TSMap>,
}

impl QueryEngine {
    pub fn new(storage: Arc<TSMap>) -> Self {
        Self { storage }
    }
    
    pub fn execute(&self, query: Query) -> Result<QueryResult, QueryError> {
        query.validate()?;
        
        let mut points = self.storage.scan_range(&query.key, query.start_time, query.end_time)?;
        
        if points.is_empty() {
            return Ok(QueryResult::Points(points));
        }
        
        if let Some(max_points) = query.max_points {
            points = downsample_points(&points, max_points);
        }
        
        if let Some(aggregation) = query.aggregation {
            let window_size = query.window_size.unwrap(); // validated above
            let aggregated = aggregate_points(&points, aggregation, window_size)?;
            Ok(QueryResult::Aggregated(aggregated))
        } else {
            Ok(QueryResult::Points(points))
        }
    }
    
    pub fn list_keys(&self) -> Vec<TimeSeriesKey> {
        self.storage.keys()
    }
    
    pub fn get_storage_stats(&self) -> storage::TSMapStats {
        self.storage.get_stats()
    }
    
    pub fn execute_multi(&self, queries: Vec<Query>) -> Result<Vec<(TimeSeriesKey, QueryResult)>, QueryError> {
        let mut results = Vec::new();
        
        for query in queries {
            let key = query.key.clone();
            let result = self.execute(query)?;
            results.push((key, result));
        }
        
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::TSMap;

    fn setup_test_data() -> Arc<TSMap> {
        let storage = Arc::new(TSMap::new());
        let key = "test.metric".to_string();
        
        for i in 0..10 {
            let point = DataPoint::new(1000 + i * 100, i as f64);
            storage.insert(key.clone(), point).unwrap();
        }
        
        storage
    }

    #[test]
    fn test_query_creation() {
        let query = Query::new("test".to_string(), 1000, 2000);
        assert_eq!(query.key, "test");
        assert_eq!(query.start_time, 1000);
        assert_eq!(query.end_time, 2000);
        assert!(query.aggregation.is_none());
    }

    #[test]
    fn test_query_with_aggregation() {
        let query = Query::new("test".to_string(), 1000, 2000)
            .with_aggregation(Aggregation::Avg, 500);
        
        assert!(query.aggregation.is_some());
        assert_eq!(query.window_size, Some(500));
    }

    #[test]
    fn test_query_validation_invalid_time_range() {
        let query = Query::new("test".to_string(), 2000, 1000);
        assert!(query.validate().is_err());
    }

    #[test]
    fn test_query_validation_aggregation_without_window() {
        let mut query = Query::new("test".to_string(), 1000, 2000);
        query.aggregation = Some(Aggregation::Sum);
        assert!(query.validate().is_err());
    }

    #[test]
    fn test_query_engine_execute_basic() {
        let storage = setup_test_data();
        let engine = QueryEngine::new(storage);
        
        let query = Query::new("test.metric".to_string(), 1200, 1600);
        let result = engine.execute(query).unwrap();
        
        match result {
            QueryResult::Points(points) => {
                assert_eq!(points.len(), 5); // 1200, 1300, 1400, 1500, 1600
            },
            _ => panic!("Expected Points result"),
        }
    }

    #[test]
    fn test_query_engine_execute_with_aggregation() {
        let storage = setup_test_data();
        let engine = QueryEngine::new(storage);
        
        let query = Query::new("test.metric".to_string(), 1000, 1900)
            .with_aggregation(Aggregation::Sum, 400);
        let result = engine.execute(query).unwrap();
        
        match result {
            QueryResult::Aggregated(points) => {
                assert!(!points.is_empty());
            },
            _ => panic!("Expected Aggregated result"),
        }
    }

    #[test]
    fn test_query_engine_execute_with_max_points() {
        let storage = setup_test_data();
        let engine = QueryEngine::new(storage);
        
        let query = Query::new("test.metric".to_string(), 1000, 1900)
            .with_max_points(3);
        let result = engine.execute(query).unwrap();
        
        match result {
            QueryResult::Points(points) => {
                assert_eq!(points.len(), 3);
            },
            _ => panic!("Expected Points result"),
        }
    }

    #[test]
    fn test_query_engine_execute_nonexistent_key() {
        let storage = setup_test_data();
        let engine = QueryEngine::new(storage);
        
        let query = Query::new("nonexistent".to_string(), 1000, 2000);
        let result = engine.execute(query);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_query_engine_list_keys() {
        let storage = setup_test_data();
        let engine = QueryEngine::new(storage);
        
        let keys = engine.list_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "test.metric");
    }

    #[test]
    fn test_query_engine_multi_query() {
        let storage = Arc::new(TSMap::new());
        
        // Setup multiple series
        for i in 0..3 {
            let key = format!("metric.{}", i);
            for j in 0..5 {
                let point = DataPoint::new(1000 + j * 100, (i * 10 + j) as f64);
                storage.insert(key.clone(), point).unwrap();
            }
        }
        
        let engine = QueryEngine::new(storage);
        
        let queries = vec![
            Query::new("metric.0".to_string(), 1000, 1400),
            Query::new("metric.1".to_string(), 1100, 1300),
        ];
        
        let results = engine.execute_multi(queries).unwrap();
        assert_eq!(results.len(), 2);
    }
}