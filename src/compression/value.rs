use crate::error::{CountError, CountResult};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

pub struct ValueCompressor {
    last_value: Option<f64>,
}

impl ValueCompressor {
    pub fn new() -> Self {
        Self { last_value: None }
    }

    pub fn compress(&mut self, value: f64) -> CountResult<Vec<u8>> {
        let mut buffer = Vec::new();
        
        match self.last_value {
            None => {
                // First value - store as-is
                buffer.write_f64::<LittleEndian>(value)?;
                self.last_value = Some(value);
            }
            Some(last_value) => {
                // XOR with previous value
                let current_bits = value.to_bits();
                let last_bits = last_value.to_bits();
                let xor_result = current_bits ^ last_bits;
                
                if xor_result == 0 {
                    // Values are identical - store single bit
                    buffer.push(0b00000001);
                } else {
                    // Find leading and trailing zeros
                    let leading_zeros = xor_result.leading_zeros();
                    let trailing_zeros = xor_result.trailing_zeros();
                    let significant_bits = 64 - leading_zeros - trailing_zeros;
                    
                    if significant_bits <= 32 {
                        // Use compact representation
                        buffer.push(0b00000010); // 2-bit prefix
                        buffer.push(leading_zeros as u8);
                        buffer.push(significant_bits as u8);
                        
                        // Store only the significant bits
                        let significant_value = xor_result >> trailing_zeros;
                        match significant_bits {
                            1..=8 => buffer.push(significant_value as u8),
                            9..=16 => buffer.write_u16::<LittleEndian>(significant_value as u16)?,
                            17..=24 => {
                                buffer.write_u16::<LittleEndian>(significant_value as u16)?;
                                buffer.push((significant_value >> 16) as u8);
                            }
                            25..=32 => buffer.write_u32::<LittleEndian>(significant_value as u32)?,
                            _ => unreachable!(),
                        }
                    } else {
                        // Fall back to full storage
                        buffer.push(0b00000100); // 3-bit prefix
                        buffer.write_u64::<LittleEndian>(xor_result)?;
                    }
                }
                
                self.last_value = Some(value);
            }
        }
        
        Ok(buffer)
    }
}

pub struct ValueDecompressor {
    last_value: Option<f64>,
}

impl ValueDecompressor {
    pub fn new() -> Self {
        Self { last_value: None }
    }

    pub fn decompress(&mut self, data: &[u8]) -> CountResult<f64> {
        let mut cursor = Cursor::new(data);
        
        match self.last_value {
            None => {
                // First value
                let value = cursor.read_f64::<LittleEndian>()?;
                self.last_value = Some(value);
                Ok(value)
            }
            Some(last_value) => {
                let prefix = cursor.read_u8()?;
                let xor_result = match prefix {
                    0b00000001 => {
                        // Values are identical
                        0u64
                    }
                    0b00000010 => {
                        // Compact representation
                        let leading_zeros = cursor.read_u8()? as u32;
                        let significant_bits = cursor.read_u8()? as u32;
                        
                        let significant_value = match significant_bits {
                            1..=8 => cursor.read_u8()? as u64,
                            9..=16 => cursor.read_u16::<LittleEndian>()? as u64,
                            17..=24 => {
                                let low = cursor.read_u16::<LittleEndian>()? as u64;
                                let high = cursor.read_u8()? as u64;
                                low | (high << 16)
                            }
                            25..=32 => cursor.read_u32::<LittleEndian>()? as u64,
                            _ => return Err(CountError::Compression {
                                message: format!("Invalid significant bits count: {}", significant_bits),
                            }),
                        };
                        
                        let trailing_zeros = 64 - leading_zeros - significant_bits;
                        significant_value << trailing_zeros
                    }
                    0b00000100 => {
                        // Full storage
                        cursor.read_u64::<LittleEndian>()?
                    }
                    _ => return Err(CountError::Compression {
                        message: format!("Invalid value compression prefix: {:#08b}", prefix),
                    }),
                };
                
                let last_bits = last_value.to_bits();
                let current_bits = last_bits ^ xor_result;
                let value = f64::from_bits(current_bits);
                
                self.last_value = Some(value);
                Ok(value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_compression_identical_values() {
        let mut compressor = ValueCompressor::new();
        let mut decompressor = ValueDecompressor::new();
        
        let values = vec![42.5, 42.5, 42.5, 42.5];
        let mut compressed_sizes = Vec::new();
        
        for &val in &values {
            let compressed = compressor.compress(val).unwrap();
            compressed_sizes.push(compressed.len());
            let decompressed = decompressor.decompress(&compressed).unwrap();
            assert_eq!(val, decompressed);
        }
        
        // First value should be 8 bytes, subsequent identical values should be 1 byte
        assert_eq!(compressed_sizes[0], 8); // Full value
        assert_eq!(compressed_sizes[1], 1); // Identical
        assert_eq!(compressed_sizes[2], 1); // Identical
        assert_eq!(compressed_sizes[3], 1); // Identical
    }

    #[test]
    fn test_value_compression_similar_values() {
        let mut compressor = ValueCompressor::new();
        let mut decompressor = ValueDecompressor::new();
        
        // Values that are similar (differ in lower bits)
        let values = vec![100.0, 100.1, 100.2, 100.3];
        
        for &val in &values {
            let compressed = compressor.compress(val).unwrap();
            let decompressed = decompressor.decompress(&compressed).unwrap();
            assert!((val - decompressed).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_value_compression_random_values() {
        let mut compressor = ValueCompressor::new();
        let mut decompressor = ValueDecompressor::new();
        
        let values = vec![3.14159, 2.71828, 1.41421, 1.61803];
        
        for &val in &values {
            let compressed = compressor.compress(val).unwrap();
            let decompressed = decompressor.decompress(&compressed).unwrap();
            assert_eq!(val, decompressed);
        }
    }

    #[test]
    fn test_value_compression_edge_cases() {
        let mut compressor = ValueCompressor::new();
        let mut decompressor = ValueDecompressor::new();
        
        let values = vec![0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN];
        
        for &val in &values[..3] { // Skip NaN for equality check
            let compressed = compressor.compress(val).unwrap();
            let decompressed = decompressor.decompress(&compressed).unwrap();
            assert_eq!(val, decompressed);
        }
        
        // Test NaN separately
        let compressed = compressor.compress(f64::NAN).unwrap();
        let decompressed = decompressor.decompress(&compressed).unwrap();
        assert!(decompressed.is_nan());
    }
}