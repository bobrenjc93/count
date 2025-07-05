use crate::error::{CountError, CountResult};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

pub struct TimestampCompressor {
    last_timestamp: Option<u64>,
    last_delta: Option<i64>,
}

impl TimestampCompressor {
    pub fn new() -> Self {
        Self {
            last_timestamp: None,
            last_delta: None,
        }
    }

    pub fn compress(&mut self, timestamp: u64) -> CountResult<Vec<u8>> {
        let mut buffer = Vec::new();
        
        match (self.last_timestamp, self.last_delta) {
            (None, None) => {
                // First timestamp - store as-is
                buffer.write_u64::<LittleEndian>(timestamp)?;
                self.last_timestamp = Some(timestamp);
            }
            (Some(last_ts), None) => {
                // Second timestamp - store delta
                let delta = timestamp as i64 - last_ts as i64;
                self.write_varint(&mut buffer, delta)?;
                self.last_timestamp = Some(timestamp);
                self.last_delta = Some(delta);
            }
            (Some(last_ts), Some(last_delta)) => {
                // Third+ timestamp - store delta-of-delta
                let delta = timestamp as i64 - last_ts as i64;
                let delta_of_delta = delta - last_delta;
                
                // Use different encoding based on delta-of-delta size
                if delta_of_delta == 0 {
                    // Most common case - same interval
                    buffer.push(0b00000001); // Single bit flag
                } else if delta_of_delta >= -63 && delta_of_delta <= 64 {
                    // Small delta-of-delta: 2-bit prefix + 7-bit value
                    buffer.push(0b00000010); // 2-bit prefix
                    buffer.push((delta_of_delta + 63) as u8);
                } else if delta_of_delta >= -255 && delta_of_delta <= 256 {
                    // Medium delta-of-delta: 3-bit prefix + 9-bit value
                    buffer.push(0b00000100); // 3-bit prefix
                    let encoded = (delta_of_delta + 255) as u16;
                    buffer.write_u16::<LittleEndian>(encoded)?;
                } else {
                    // Large delta-of-delta: 4-bit prefix + full value
                    buffer.push(0b00001000); // 4-bit prefix
                    buffer.write_i64::<LittleEndian>(delta_of_delta)?;
                }
                
                self.last_timestamp = Some(timestamp);
                self.last_delta = Some(delta);
            }
            (None, Some(_)) => {
                // This should never happen in normal operation
                return Err(CountError::Compression {
                    message: "Invalid state: no timestamp but has delta".to_string(),
                });
            }
        }
        
        Ok(buffer)
    }

    fn write_varint(&self, buffer: &mut Vec<u8>, mut value: i64) -> CountResult<()> {
        let sign = if value < 0 { 1u8 } else { 0u8 };
        if value < 0 {
            value = -value;
        }
        
        buffer.push(sign);
        
        while value >= 128 {
            buffer.push((value & 0x7F) as u8 | 0x80);
            value >>= 7;
        }
        buffer.push(value as u8);
        
        Ok(())
    }
}

pub struct TimestampDecompressor {
    last_timestamp: Option<u64>,
    last_delta: Option<i64>,
}

impl TimestampDecompressor {
    pub fn new() -> Self {
        Self {
            last_timestamp: None,
            last_delta: None,
        }
    }

    pub fn decompress(&mut self, data: &[u8]) -> CountResult<u64> {
        let mut cursor = Cursor::new(data);
        
        match (self.last_timestamp, self.last_delta) {
            (None, None) => {
                // First timestamp
                let timestamp = cursor.read_u64::<LittleEndian>()?;
                self.last_timestamp = Some(timestamp);
                Ok(timestamp)
            }
            (Some(_), None) => {
                // Second timestamp - read delta
                let delta = self.read_varint(&mut cursor)?;
                let timestamp = (self.last_timestamp.unwrap() as i64 + delta) as u64;
                self.last_timestamp = Some(timestamp);
                self.last_delta = Some(delta);
                Ok(timestamp)
            }
            (Some(last_ts), Some(last_delta)) => {
                // Third+ timestamp - read delta-of-delta
                let prefix = cursor.read_u8()?;
                let delta_of_delta = match prefix {
                    0b00000001 => 0, // Same interval
                    0b00000010 => {
                        // Small delta-of-delta
                        let encoded = cursor.read_u8()? as i64;
                        encoded - 63
                    }
                    0b00000100 => {
                        // Medium delta-of-delta
                        let encoded = cursor.read_u16::<LittleEndian>()? as i64;
                        encoded - 255
                    }
                    0b00001000 => {
                        // Large delta-of-delta
                        cursor.read_i64::<LittleEndian>()?
                    }
                    _ => return Err(CountError::Compression {
                        message: format!("Invalid timestamp compression prefix: {:#08b}", prefix),
                    }),
                };
                
                let delta = last_delta + delta_of_delta;
                let timestamp = (last_ts as i64 + delta) as u64;
                
                self.last_timestamp = Some(timestamp);
                self.last_delta = Some(delta);
                
                Ok(timestamp)
            }
            (None, Some(_)) => {
                // This should never happen in normal operation
                Err(CountError::Compression {
                    message: "Invalid state: no timestamp but has delta".to_string(),
                })
            }
        }
    }

    fn read_varint(&self, cursor: &mut Cursor<&[u8]>) -> CountResult<i64> {
        let sign = cursor.read_u8()?;
        let mut value = 0i64;
        let mut shift = 0;
        
        loop {
            let byte = cursor.read_u8()?;
            value |= ((byte & 0x7F) as i64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        
        if sign == 1 {
            value = -value;
        }
        
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_compression_regular_intervals() {
        let mut compressor = TimestampCompressor::new();
        let mut decompressor = TimestampDecompressor::new();
        
        let timestamps = vec![1000, 1060, 1120, 1180, 1240]; // 60-second intervals
        let mut compressed_sizes = Vec::new();
        
        for &ts in &timestamps {
            let compressed = compressor.compress(ts).unwrap();
            compressed_sizes.push(compressed.len());
            let decompressed = decompressor.decompress(&compressed).unwrap();
            assert_eq!(ts, decompressed);
        }
        
        // First timestamp should be 8 bytes, subsequent should be much smaller
        assert_eq!(compressed_sizes[0], 8); // Full timestamp
        assert!(compressed_sizes[1] > 1); // Delta
        assert_eq!(compressed_sizes[2], 1); // Delta-of-delta = 0 (regular interval)
        assert_eq!(compressed_sizes[3], 1); // Delta-of-delta = 0 (regular interval)
        assert_eq!(compressed_sizes[4], 1); // Delta-of-delta = 0 (regular interval)
    }

    #[test]
    fn test_timestamp_compression_irregular_intervals() {
        let mut compressor = TimestampCompressor::new();
        let mut decompressor = TimestampDecompressor::new();
        
        let timestamps = vec![1000, 1063, 1119, 1185, 1241]; // Irregular intervals
        
        for &ts in &timestamps {
            let compressed = compressor.compress(ts).unwrap();
            let decompressed = decompressor.decompress(&compressed).unwrap();
            assert_eq!(ts, decompressed);
        }
    }
}