use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompressionError {
    #[error("Invalid bit length: {0}")]
    InvalidBitLength(usize),
    
    #[error("Buffer overflow")]
    BufferOverflow,
    
    #[error("Insufficient data for decompression")]
    InsufficientData,
    
    #[error("Invalid compressed format")]
    InvalidFormat,
}