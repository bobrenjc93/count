use crate::{DataPoint, error::{CountError, CountResult}};
use crate::storage::memory::CompressedBlock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs as async_fs;

#[derive(Debug, Serialize, Deserialize)]
struct DiskBlock {
    start_time: u64,
    end_time: u64,
    compressed_timestamps: Vec<u8>,
    compressed_values: Vec<u8>,
    point_count: usize,
    original_points: Vec<DataPoint>,
}

impl From<CompressedBlock> for DiskBlock {
    fn from(block: CompressedBlock) -> Self {
        Self {
            start_time: block.start_time,
            end_time: block.end_time,
            compressed_timestamps: block.compressed_timestamps,
            compressed_values: block.compressed_values,
            point_count: block.point_count,
            original_points: block.original_points,
        }
    }
}

impl From<DiskBlock> for CompressedBlock {
    fn from(disk_block: DiskBlock) -> Self {
        Self {
            start_time: disk_block.start_time,
            end_time: disk_block.end_time,
            compressed_timestamps: disk_block.compressed_timestamps,
            compressed_values: disk_block.compressed_values,
            point_count: disk_block.point_count,
            original_points: disk_block.original_points,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SeriesManifest {
    series_name: String,
    blocks: Vec<BlockMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BlockMetadata {
    file_name: String,
    start_time: u64,
    end_time: u64,
    point_count: usize,
}

pub struct DiskStorage {
    data_dir: PathBuf,
}

impl DiskStorage {
    pub async fn new<P: AsRef<Path>>(data_dir: P) -> CountResult<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        
        // Create data directory if it doesn't exist
        if !data_dir.exists() {
            async_fs::create_dir_all(&data_dir).await?;
        }
        
        Ok(Self { data_dir })
    }

    pub async fn flush_blocks(&self, series_blocks: HashMap<String, Vec<CompressedBlock>>) -> CountResult<()> {
        for (series_name, blocks) in series_blocks {
            self.flush_series_blocks(&series_name, blocks).await?;
        }
        Ok(())
    }

    async fn flush_series_blocks(&self, series_name: &str, blocks: Vec<CompressedBlock>) -> CountResult<()> {
        let series_dir = self.data_dir.join(series_name);
        if !series_dir.exists() {
            async_fs::create_dir_all(&series_dir).await?;
        }

        let mut manifest = self.load_series_manifest(series_name).await.unwrap_or_else(|_| SeriesManifest {
            series_name: series_name.to_string(),
            blocks: Vec::new(),
        });

        for block in blocks {
            let file_name = format!("block_{}_{}.json", block.start_time, block.end_time);
            let file_path = series_dir.join(&file_name);
            
            let disk_block = DiskBlock::from(block.clone());
            let serialized = serde_json::to_vec(&disk_block)?;
            
            async_fs::write(&file_path, serialized).await?;
            
            manifest.blocks.push(BlockMetadata {
                file_name,
                start_time: block.start_time,
                end_time: block.end_time,
                point_count: block.point_count,
            });
        }

        // Save updated manifest
        self.save_series_manifest(&manifest).await?;
        
        Ok(())
    }

    pub async fn load_blocks_for_range(&self, series_name: &str, start_time: u64, end_time: u64) -> CountResult<Vec<CompressedBlock>> {
        let manifest = self.load_series_manifest(series_name).await?;
        let mut blocks = Vec::new();
        
        let series_dir = self.data_dir.join(series_name);
        
        for block_meta in &manifest.blocks {
            // Check if block overlaps with query range
            if block_meta.end_time >= start_time && block_meta.start_time <= end_time {
                let file_path = series_dir.join(&block_meta.file_name);
                
                if file_path.exists() {
                    let data = async_fs::read(&file_path).await?;
                    let disk_block: DiskBlock = serde_json::from_slice(&data)?;
                    blocks.push(CompressedBlock::from(disk_block));
                }
            }
        }
        
        Ok(blocks)
    }

    async fn load_series_manifest(&self, series_name: &str) -> CountResult<SeriesManifest> {
        let manifest_path = self.data_dir.join(series_name).join("manifest.json");
        
        if !manifest_path.exists() {
            return Err(CountError::Storage {
                message: format!("Manifest not found for series: {}", series_name),
            });
        }
        
        let data = async_fs::read(&manifest_path).await?;
        let manifest: SeriesManifest = serde_json::from_slice(&data)?;
        
        Ok(manifest)
    }

    async fn save_series_manifest(&self, manifest: &SeriesManifest) -> CountResult<()> {
        let series_dir = self.data_dir.join(&manifest.series_name);
        let manifest_path = series_dir.join("manifest.json");
        
        let serialized = serde_json::to_vec_pretty(manifest)?;
        async_fs::write(&manifest_path, serialized).await?;
        
        Ok(())
    }

    pub async fn get_series_list(&self) -> CountResult<Vec<String>> {
        let mut series_list = Vec::new();
        
        if !self.data_dir.exists() {
            return Ok(series_list);
        }
        
        let mut dir_entries = async_fs::read_dir(&self.data_dir).await?;
        
        while let Some(entry) = dir_entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(series_name) = entry.file_name().to_str() {
                    let manifest_path = entry.path().join("manifest.json");
                    if manifest_path.exists() {
                        series_list.push(series_name.to_string());
                    }
                }
            }
        }
        
        Ok(series_list)
    }

    pub async fn cleanup_old_data(&self, cutoff_time: u64) -> CountResult<usize> {
        let series_list = self.get_series_list().await?;
        let mut cleaned_blocks = 0;
        
        for series_name in series_list {
            cleaned_blocks += self.cleanup_series_old_data(&series_name, cutoff_time).await?;
        }
        
        Ok(cleaned_blocks)
    }

    async fn cleanup_series_old_data(&self, series_name: &str, cutoff_time: u64) -> CountResult<usize> {
        let mut manifest = self.load_series_manifest(series_name).await?;
        let series_dir = self.data_dir.join(series_name);
        
        let mut cleaned_blocks = 0;
        let mut remaining_blocks = Vec::new();
        
        for block_meta in manifest.blocks {
            if block_meta.end_time < cutoff_time {
                // Delete old block file
                let file_path = series_dir.join(&block_meta.file_name);
                if file_path.exists() {
                    async_fs::remove_file(&file_path).await?;
                    cleaned_blocks += 1;
                }
            } else {
                remaining_blocks.push(block_meta);
            }
        }
        
        // Update manifest with remaining blocks
        manifest.blocks = remaining_blocks;
        self.save_series_manifest(&manifest).await?;
        
        Ok(cleaned_blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::CompressedBlock;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_disk_storage_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DiskStorage::new(temp_dir.path()).await.unwrap();
        
        // Create test blocks
        let mut blocks = HashMap::new();
        let test_block = CompressedBlock {
            start_time: 1000,
            end_time: 2000,
            compressed_timestamps: vec![1, 2, 3],
            compressed_values: vec![4, 5, 6],
            point_count: 3,
            original_points: Vec::new(),
        };
        blocks.insert("test.series".to_string(), vec![test_block]);
        
        // Flush to disk
        storage.flush_blocks(blocks).await.unwrap();
        
        // Load back from disk
        let loaded_blocks = storage.load_blocks_for_range("test.series", 500, 2500).await.unwrap();
        assert_eq!(loaded_blocks.len(), 1);
        assert_eq!(loaded_blocks[0].start_time, 1000);
        assert_eq!(loaded_blocks[0].point_count, 3);
    }

    #[tokio::test]
    async fn test_disk_storage_series_list() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DiskStorage::new(temp_dir.path()).await.unwrap();
        
        // Create test data for multiple series
        let mut blocks = HashMap::new();
        let test_block = CompressedBlock::new(1000);
        blocks.insert("series1".to_string(), vec![test_block.clone()]);
        blocks.insert("series2".to_string(), vec![test_block]);
        
        storage.flush_blocks(blocks).await.unwrap();
        
        let series_list = storage.get_series_list().await.unwrap();
        assert_eq!(series_list.len(), 2);
        assert!(series_list.contains(&"series1".to_string()));
        assert!(series_list.contains(&"series2".to_string()));
    }
}