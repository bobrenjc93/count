use crate::error::CompressionError;

pub struct TimestampCompressor {
    last_timestamp: u64,
    last_delta: i64,
}

impl TimestampCompressor {
    pub fn new(first_timestamp: u64) -> Self {
        Self {
            last_timestamp: first_timestamp,
            last_delta: 0,
        }
    }
    
    pub fn compress(&mut self, timestamp: u64, writer: &mut BitWriter) -> Result<(), CompressionError> {
        let delta = timestamp as i64 - self.last_timestamp as i64;
        let delta_of_delta = delta - self.last_delta;
        
        if delta_of_delta == 0 {
            writer.write_bit(0)?;
        } else if delta_of_delta >= -63 && delta_of_delta <= 64 {
            writer.write_bits(0b10, 2)?;
            writer.write_bits((delta_of_delta as u8 as u64) & 0x7F, 7)?;
        } else if delta_of_delta >= -255 && delta_of_delta <= 256 {
            writer.write_bits(0b110, 3)?;
            writer.write_bits((delta_of_delta as u16 as u64) & 0x1FF, 9)?;
        } else if delta_of_delta >= -2047 && delta_of_delta <= 2048 {
            writer.write_bits(0b1110, 4)?;
            writer.write_bits((delta_of_delta as u16 as u64) & 0xFFF, 12)?;
        } else {
            writer.write_bits(0b1111, 4)?;
            writer.write_bits(delta_of_delta as u32 as u64, 32)?;
        }
        
        self.last_timestamp = timestamp;
        self.last_delta = delta;
        Ok(())
    }
}

pub struct TimestampDecompressor {
    last_timestamp: u64,
    last_delta: i64,
}

impl TimestampDecompressor {
    pub fn new(first_timestamp: u64) -> Self {
        Self {
            last_timestamp: first_timestamp,
            last_delta: 0,
        }
    }
    
    pub fn decompress(&mut self, reader: &mut BitReader) -> Result<u64, CompressionError> {
        let delta_of_delta = if reader.read_bit()? == 0 {
            0
        } else {
            let next_bit = reader.read_bit()?;
            if next_bit == 0 {
                let bits = reader.read_bits(7)? as u8;
                (bits as i8) as i64
            } else {
                let next_bit = reader.read_bit()?;
                if next_bit == 0 {
                    let bits = reader.read_bits(9)? as u16;
                    if bits & 0x100 != 0 {
                        ((bits | 0xFE00) as i16) as i64
                    } else {
                        (bits as i16) as i64
                    }
                } else {
                    let next_bit = reader.read_bit()?;
                    if next_bit == 0 {
                        let bits = reader.read_bits(12)? as u16;
                        if bits & 0x800 != 0 {
                            ((bits | 0xF000) as i16) as i64
                        } else {
                            (bits as i16) as i64
                        }
                    } else {
                        reader.read_bits(32)? as i32 as i64
                    }
                }
            }
        };
        
        let delta = self.last_delta + delta_of_delta;
        let timestamp = (self.last_timestamp as i64 + delta) as u64;
        
        self.last_timestamp = timestamp;
        self.last_delta = delta;
        
        Ok(timestamp)
    }
}

pub struct BitWriter {
    buffer: Vec<u8>,
    bit_count: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            bit_count: 0,
        }
    }
    
    pub fn write_bit(&mut self, bit: u8) -> Result<(), CompressionError> {
        if self.bit_count % 8 == 0 {
            self.buffer.push(0);
        }
        
        let byte_index = self.bit_count / 8;
        let bit_index = 7 - (self.bit_count % 8);
        
        if bit != 0 {
            self.buffer[byte_index] |= 1 << bit_index;
        }
        
        self.bit_count += 1;
        Ok(())
    }
    
    pub fn write_bits(&mut self, value: u64, num_bits: usize) -> Result<(), CompressionError> {
        for i in (0..num_bits).rev() {
            let bit = ((value >> i) & 1) as u8;
            self.write_bit(bit)?;
        }
        Ok(())
    }
    
    pub fn finish(self) -> Vec<u8> {
        self.buffer
    }
    
    pub fn bit_count(&self) -> usize {
        self.bit_count
    }
}

pub struct BitReader {
    buffer: Vec<u8>,
    bit_position: usize,
}

impl BitReader {
    pub fn new(buffer: Vec<u8>) -> Self {
        Self {
            buffer,
            bit_position: 0,
        }
    }
    
    pub fn read_bit(&mut self) -> Result<u8, CompressionError> {
        if self.bit_position >= self.buffer.len() * 8 {
            return Err(CompressionError::InsufficientData);
        }
        
        let byte_index = self.bit_position / 8;
        let bit_index = 7 - (self.bit_position % 8);
        
        let bit = (self.buffer[byte_index] >> bit_index) & 1;
        self.bit_position += 1;
        
        Ok(bit)
    }
    
    pub fn read_bits(&mut self, num_bits: usize) -> Result<u64, CompressionError> {
        let mut value = 0u64;
        
        for _ in 0..num_bits {
            value = (value << 1) | (self.read_bit()? as u64);
        }
        
        Ok(value)
    }
    
    pub fn peek_bits(&self, num_bits: usize) -> Result<u64, CompressionError> {
        let mut value = 0u64;
        let mut pos = self.bit_position;
        
        for _ in 0..num_bits {
            if pos >= self.buffer.len() * 8 {
                return Err(CompressionError::InsufficientData);
            }
            
            let byte_index = pos / 8;
            let bit_index = 7 - (pos % 8);
            let bit = (self.buffer[byte_index] >> bit_index) & 1;
            
            value = (value << 1) | (bit as u64);
            pos += 1;
        }
        
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_writer_single_bit() {
        let mut writer = BitWriter::new();
        writer.write_bit(1).unwrap();
        writer.write_bit(0).unwrap();
        writer.write_bit(1).unwrap();
        
        let data = writer.finish();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], 0b10100000);
    }

    #[test]
    fn test_bit_writer_multiple_bytes() {
        let mut writer = BitWriter::new();
        for i in 0..16 {
            writer.write_bit(i % 2).unwrap();
        }
        
        let data = writer.finish();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0], 0b01010101);
        assert_eq!(data[1], 0b01010101);
    }

    #[test]
    fn test_bit_writer_bits() {
        let mut writer = BitWriter::new();
        writer.write_bits(0b101, 3).unwrap();
        writer.write_bits(0b11, 2).unwrap();
        
        let data = writer.finish();
        assert_eq!(data[0], 0b10111000);
    }

    #[test]
    fn test_bit_reader_single_bit() {
        let data = vec![0b10100000];
        let mut reader = BitReader::new(data);
        
        assert_eq!(reader.read_bit().unwrap(), 1);
        assert_eq!(reader.read_bit().unwrap(), 0);
        assert_eq!(reader.read_bit().unwrap(), 1);
        assert_eq!(reader.read_bit().unwrap(), 0);
    }

    #[test]
    fn test_bit_reader_bits() {
        let data = vec![0b10111000];
        let mut reader = BitReader::new(data);
        
        assert_eq!(reader.read_bits(3).unwrap(), 0b101);
        assert_eq!(reader.read_bits(2).unwrap(), 0b11);
    }

    #[test]
    fn test_timestamp_compression_zero_delta() {
        let mut compressor = TimestampCompressor::new(1000);
        let mut writer = BitWriter::new();
        
        compressor.compress(1010, &mut writer).unwrap();
        compressor.compress(1020, &mut writer).unwrap();
        compressor.compress(1030, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = TimestampDecompressor::new(1000);
        
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1010);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1020);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1030);
    }

    #[test]
    fn test_timestamp_compression_small_delta() {
        let mut compressor = TimestampCompressor::new(1000);
        let mut writer = BitWriter::new();
        
        compressor.compress(1010, &mut writer).unwrap();
        compressor.compress(1021, &mut writer).unwrap();
        compressor.compress(1032, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = TimestampDecompressor::new(1000);
        
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1010);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1021);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1032);
    }

    #[test]
    fn test_timestamp_compression_large_delta() {
        let mut compressor = TimestampCompressor::new(1000);
        let mut writer = BitWriter::new();
        
        compressor.compress(1010, &mut writer).unwrap();
        compressor.compress(2000, &mut writer).unwrap();
        compressor.compress(2010, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = TimestampDecompressor::new(1000);
        
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1010);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 2000);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 2010);
    }

    #[test]
    fn test_empty_data_error() {
        let data = vec![];
        let mut reader = BitReader::new(data);
        
        assert!(reader.read_bit().is_err());
    }
}