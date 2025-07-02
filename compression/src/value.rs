use crate::error::CompressionError;
use crate::timestamp::{BitWriter, BitReader};

pub struct ValueCompressor {
    last_value: f64,
    last_leading_zeros: usize,
    last_trailing_zeros: usize,
}

impl ValueCompressor {
    pub fn new(first_value: f64) -> Self {
        Self {
            last_value: first_value,
            last_leading_zeros: 0,
            last_trailing_zeros: 0,
        }
    }
    
    pub fn compress(&mut self, value: f64, writer: &mut BitWriter) -> Result<(), CompressionError> {
        let current_bits = value.to_bits();
        let last_bits = self.last_value.to_bits();
        
        if current_bits == last_bits {
            writer.write_bit(0)?;
            return Ok(());
        }
        
        writer.write_bit(1)?;
        
        let xor = current_bits ^ last_bits;
        let leading_zeros = xor.leading_zeros() as usize;
        let trailing_zeros = xor.trailing_zeros() as usize;
        
        if leading_zeros >= self.last_leading_zeros 
            && trailing_zeros >= self.last_trailing_zeros {
            writer.write_bit(0)?;
            
            let meaningful_bits = 64 - self.last_leading_zeros - self.last_trailing_zeros;
            let shifted_xor = xor >> self.last_trailing_zeros;
            writer.write_bits(shifted_xor, meaningful_bits)?;
        } else {
            writer.write_bit(1)?;
            
            writer.write_bits(leading_zeros as u64, 5)?;
            
            let meaningful_bits = 64 - leading_zeros - trailing_zeros;
            writer.write_bits(meaningful_bits as u64, 6)?;
            
            let shifted_xor = xor >> trailing_zeros;
            writer.write_bits(shifted_xor, meaningful_bits)?;
            
            self.last_leading_zeros = leading_zeros;
            self.last_trailing_zeros = trailing_zeros;
        }
        
        self.last_value = value;
        Ok(())
    }
}

pub struct ValueDecompressor {
    last_value: f64,
    last_leading_zeros: usize,
    last_trailing_zeros: usize,
}

impl ValueDecompressor {
    pub fn new(first_value: f64) -> Self {
        Self {
            last_value: first_value,
            last_leading_zeros: 0,
            last_trailing_zeros: 0,
        }
    }
    
    pub fn decompress(&mut self, reader: &mut BitReader) -> Result<f64, CompressionError> {
        if reader.read_bit()? == 0 {
            return Ok(self.last_value);
        }
        
        let xor = if reader.read_bit()? == 0 {
            let meaningful_bits = 64 - self.last_leading_zeros - self.last_trailing_zeros;
            let shifted_xor = reader.read_bits(meaningful_bits)?;
            shifted_xor << self.last_trailing_zeros
        } else {
            let leading_zeros = reader.read_bits(5)? as usize;
            let meaningful_bits = reader.read_bits(6)? as usize;
            let trailing_zeros = 64 - leading_zeros - meaningful_bits;
            
            let shifted_xor = reader.read_bits(meaningful_bits)?;
            
            self.last_leading_zeros = leading_zeros;
            self.last_trailing_zeros = trailing_zeros;
            
            shifted_xor << trailing_zeros
        };
        
        let current_bits = self.last_value.to_bits() ^ xor;
        let value = f64::from_bits(current_bits);
        
        self.last_value = value;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_compression_same_value() {
        let mut compressor = ValueCompressor::new(42.5);
        let mut writer = BitWriter::new();
        
        compressor.compress(42.5, &mut writer).unwrap();
        compressor.compress(42.5, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = ValueDecompressor::new(42.5);
        
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 42.5);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 42.5);
    }

    #[test]
    fn test_value_compression_different_values() {
        let mut compressor = ValueCompressor::new(42.5);
        let mut writer = BitWriter::new();
        
        compressor.compress(43.0, &mut writer).unwrap();
        compressor.compress(44.5, &mut writer).unwrap();
        compressor.compress(45.0, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = ValueDecompressor::new(42.5);
        
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 43.0);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 44.5);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 45.0);
    }

    #[test]
    fn test_value_compression_similar_patterns() {
        let mut compressor = ValueCompressor::new(1.0);
        let mut writer = BitWriter::new();
        
        compressor.compress(1.1, &mut writer).unwrap();
        compressor.compress(1.2, &mut writer).unwrap();
        compressor.compress(1.3, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = ValueDecompressor::new(1.0);
        
        assert!((decompressor.decompress(&mut reader).unwrap() - 1.1).abs() < 1e-10);
        assert!((decompressor.decompress(&mut reader).unwrap() - 1.2).abs() < 1e-10);
        assert!((decompressor.decompress(&mut reader).unwrap() - 1.3).abs() < 1e-10);
    }

    #[test]
    fn test_value_compression_zero_values() {
        let mut compressor = ValueCompressor::new(0.0);
        let mut writer = BitWriter::new();
        
        compressor.compress(0.0, &mut writer).unwrap();
        compressor.compress(1.0, &mut writer).unwrap();
        compressor.compress(0.0, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = ValueDecompressor::new(0.0);
        
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 0.0);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 1.0);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), 0.0);
    }

    #[test]
    fn test_value_compression_special_values() {
        let mut compressor = ValueCompressor::new(f64::NAN);
        let mut writer = BitWriter::new();
        
        compressor.compress(f64::INFINITY, &mut writer).unwrap();
        compressor.compress(f64::NEG_INFINITY, &mut writer).unwrap();
        
        let data = writer.finish();
        let mut reader = BitReader::new(data);
        let mut decompressor = ValueDecompressor::new(f64::NAN);
        
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), f64::INFINITY);
        assert_eq!(decompressor.decompress(&mut reader).unwrap(), f64::NEG_INFINITY);
    }
}