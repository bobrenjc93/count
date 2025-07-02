use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsdbError {
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(u64),
    
    #[error("Invalid value: {0}")]
    InvalidValue(f64),
    
    #[error("Time series not found: {0}")]
    TimeSeriesNotFound(String),
    
    #[error("Compression error: {0}")]
    CompressionError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Replication error: {0}")]
    ReplicationError(String),
}