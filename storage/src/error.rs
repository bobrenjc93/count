use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    
    #[error("Block is sealed and cannot be modified")]
    BlockSealed,
    
    #[error("Invalid time range: start {0} > end {1}")]
    InvalidTimeRange(u64, u64),
    
    #[error("Compression error: {0}")]
    CompressionError(#[from] compression::CompressionError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}