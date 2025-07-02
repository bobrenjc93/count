use tsdb_core::{TimeSeriesKey, DataPoint};
use crate::error::StorageError;
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io::{Write, Read, BufReader, Seek, SeekFrom};
use bincode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALEntry {
    pub key: TimeSeriesKey,
    pub point: DataPoint,
    pub timestamp: u64, // WAL entry timestamp
}

impl WALEntry {
    pub fn new(key: TimeSeriesKey, point: DataPoint) -> Self {
        Self {
            key,
            point,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}

pub struct WriteAheadLog {
    file: File,
    path: PathBuf,
    entry_count: usize,
}

impl WriteAheadLog {
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
            
        Ok(Self {
            file,
            path,
            entry_count: 0,
        })
    }
    
    pub fn append(&mut self, entry: WALEntry) -> Result<(), StorageError> {
        let encoded = bincode::serialize(&entry)
            .map_err(|e| StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Serialization error: {}", e)
            )))?;
            
        // Write length prefix
        let len = encoded.len() as u32;
        self.file.write_all(&len.to_le_bytes())?;
        
        // Write entry data
        self.file.write_all(&encoded)?;
        self.file.flush()?;
        
        self.entry_count += 1;
        Ok(())
    }
    
    pub fn replay<F>(&self, mut callback: F) -> Result<usize, StorageError>
    where
        F: FnMut(WALEntry) -> Result<(), StorageError>,
    {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        let mut count = 0;
        
        loop {
            // Read length prefix
            let mut len_bytes = [0u8; 4];
            match reader.read_exact(&mut len_bytes) {
                Ok(()) => {},
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(StorageError::IoError(e)),
            }
            
            let len = u32::from_le_bytes(len_bytes) as usize;
            
            // Read entry data
            let mut entry_bytes = vec![0u8; len];
            reader.read_exact(&mut entry_bytes)?;
            
            let entry: WALEntry = bincode::deserialize(&entry_bytes)
                .map_err(|e| StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Deserialization error: {}", e)
                )))?;
                
            callback(entry)?;
            count += 1;
        }
        
        Ok(count)
    }
    
    pub fn truncate(&mut self) -> Result<(), StorageError> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.entry_count = 0;
        Ok(())
    }
    
    pub fn entry_count(&self) -> usize {
        self.entry_count
    }
    
    pub fn sync(&mut self) -> Result<(), StorageError> {
        self.file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_wal_create() {
        let temp_file = NamedTempFile::new().unwrap();
        let wal = WriteAheadLog::create(temp_file.path()).unwrap();
        assert_eq!(wal.entry_count(), 0);
    }

    #[test]
    fn test_wal_append_and_replay() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut wal = WriteAheadLog::create(temp_file.path()).unwrap();
        
        let entries = vec![
            WALEntry::new("metric1".to_string(), DataPoint::new(1000, 42.0)),
            WALEntry::new("metric2".to_string(), DataPoint::new(2000, 84.0)),
            WALEntry::new("metric1".to_string(), DataPoint::new(3000, 126.0)),
        ];
        
        for entry in &entries {
            wal.append(entry.clone()).unwrap();
        }
        
        assert_eq!(wal.entry_count(), 3);
        
        let mut replayed_entries = Vec::new();
        let count = wal.replay(|entry| {
            replayed_entries.push(entry);
            Ok(())
        }).unwrap();
        
        assert_eq!(count, 3);
        assert_eq!(replayed_entries.len(), 3);
        
        for (original, replayed) in entries.iter().zip(replayed_entries.iter()) {
            assert_eq!(original.key, replayed.key);
            assert_eq!(original.point.timestamp, replayed.point.timestamp);
            assert_eq!(original.point.value, replayed.point.value);
        }
    }

    #[test]
    fn test_wal_truncate() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut wal = WriteAheadLog::create(temp_file.path()).unwrap();
        
        let entry = WALEntry::new("metric1".to_string(), DataPoint::new(1000, 42.0));
        wal.append(entry).unwrap();
        assert_eq!(wal.entry_count(), 1);
        
        wal.truncate().unwrap();
        assert_eq!(wal.entry_count(), 0);
        
        let count = wal.replay(|_| Ok(())).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_wal_empty_replay() {
        let temp_file = NamedTempFile::new().unwrap();
        let wal = WriteAheadLog::create(temp_file.path()).unwrap();
        
        let count = wal.replay(|_| Ok(())).unwrap();
        assert_eq!(count, 0);
    }
}