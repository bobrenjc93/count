use thiserror::Error;

#[derive(Error, Debug)]
pub enum CountError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Storage error: {message}")]
    Storage { message: String },
    
    #[error("Compression error: {message}")]
    Compression { message: String },
    
    #[error("Query error: {message}")]
    Query { message: String },
    
    #[error("Network error: {0}")]
    NetworkError(String),
}

pub type CountResult<T> = Result<T, CountError>;