use thiserror::Error;

#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
    
    #[error("Time series not found: {0}")]
    TimeSeriesNotFound(String),
    
    #[error("Invalid time range: start {0} > end {1}")]
    InvalidTimeRange(u64, u64),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] storage::StorageError),
    
    #[error("Aggregation error: {0}")]
    AggregationError(String),
}