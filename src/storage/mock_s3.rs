use crate::{error::{CountError, CountResult}};
use crate::storage::memory::CompressedBlock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize, Deserialize)]
struct MockS3Block {
    start_time: u64,
    end_time: u64,
    compressed_timestamps: Vec<u8>,
    compressed_values: Vec<u8>,
    point_count: usize,
}

impl From<CompressedBlock> for MockS3Block {
    fn from(block: CompressedBlock) -> Self {
        Self {
            start_time: block.start_time,
            end_time: block.end_time,
            compressed_timestamps: block.compressed_timestamps,
            compressed_values: block.compressed_values,
            point_count: block.point_count,
        }
    }
}

impl From<MockS3Block> for CompressedBlock {
    fn from(s3_block: MockS3Block) -> Self {
        Self {
            start_time: s3_block.start_time,
            end_time: s3_block.end_time,
            compressed_timestamps: s3_block.compressed_timestamps,
            compressed_values: s3_block.compressed_values,
            point_count: s3_block.point_count,
            original_points: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MockS3SeriesManifest {
    series_name: String,
    blocks: Vec<MockS3BlockMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MockS3BlockMetadata {
    key: String,
    start_time: u64,
    end_time: u64,
    point_count: usize,
}

// Mock S3 storage that stores data in memory for testing
pub struct MockS3Storage {
    _bucket: String,
    prefix: String,
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl MockS3Storage {
    pub fn new(bucket: String, prefix: Option<String>) -> Self {
        Self {
            _bucket: bucket,
            prefix: prefix.unwrap_or_else(|| "count-data".to_string()),
            objects: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn archive_blocks(&self, series_blocks: HashMap<String, Vec<CompressedBlock>>) -> CountResult<()> {
        for (series_name, blocks) in series_blocks {
            self.archive_series_blocks(&series_name, blocks)?;
        }
        Ok(())
    }

    fn archive_series_blocks(&self, series_name: &str, blocks: Vec<CompressedBlock>) -> CountResult<()> {
        let mut manifest = self.load_series_manifest(series_name).unwrap_or_else(|_| MockS3SeriesManifest {
            series_name: series_name.to_string(),
            blocks: Vec::new(),
        });

        for block in blocks {
            let key = format!("{}/{}/block_{}_{}.json", self.prefix, series_name, block.start_time, block.end_time);
            
            let s3_block = MockS3Block::from(block.clone());
            let serialized = serde_json::to_vec(&s3_block)?;
            
            // Store in mock HashMap
            {
                let mut objects = self.objects.lock().unwrap();
                objects.insert(key.clone(), serialized);
            }
            
            manifest.blocks.push(MockS3BlockMetadata {
                key: key.clone(),
                start_time: block.start_time,
                end_time: block.end_time,
                point_count: block.point_count,
            });
        }

        self.save_series_manifest(&manifest)?;
        Ok(())
    }

    pub fn load_blocks_for_range(&self, series_name: &str, start_time: u64, end_time: u64) -> CountResult<Vec<CompressedBlock>> {
        let manifest = self.load_series_manifest(series_name)?;
        let mut blocks = Vec::new();
        
        let objects = self.objects.lock().unwrap();
        
        for block_meta in &manifest.blocks {
            if block_meta.end_time >= start_time && block_meta.start_time <= end_time {
                if let Some(data) = objects.get(&block_meta.key) {
                    let s3_block: MockS3Block = serde_json::from_slice(data)?;
                    blocks.push(CompressedBlock::from(s3_block));
                }
            }
        }
        
        Ok(blocks)
    }

    fn load_series_manifest(&self, series_name: &str) -> CountResult<MockS3SeriesManifest> {
        let manifest_key = format!("{}/{}/manifest.json", self.prefix, series_name);
        
        let objects = self.objects.lock().unwrap();
        let data = objects.get(&manifest_key).ok_or_else(|| CountError::Storage {
            message: format!("Manifest not found for series {}", series_name),
        })?;
        
        let manifest: MockS3SeriesManifest = serde_json::from_slice(data)?;
        Ok(manifest)
    }

    fn save_series_manifest(&self, manifest: &MockS3SeriesManifest) -> CountResult<()> {
        let manifest_key = format!("{}/{}/manifest.json", self.prefix, &manifest.series_name);
        let serialized = serde_json::to_vec_pretty(manifest)?;
        
        let mut objects = self.objects.lock().unwrap();
        objects.insert(manifest_key, serialized);
        
        Ok(())
    }

    pub fn get_series_list(&self) -> CountResult<Vec<String>> {
        let objects = self.objects.lock().unwrap();
        let list_prefix = format!("{}/", self.prefix);
        
        let mut series_set = std::collections::HashSet::new();
        
        for key in objects.keys() {
            if key.starts_with(&list_prefix) && key.ends_with("/manifest.json") {
                let series_name = key
                    .strip_prefix(&list_prefix)
                    .and_then(|s| s.strip_suffix("/manifest.json"))
                    .unwrap_or("")
                    .to_string();
                
                if !series_name.is_empty() {
                    series_set.insert(series_name);
                }
            }
        }
        
        Ok(series_set.into_iter().collect())
    }

    pub fn cleanup_old_data(&self, cutoff_time: u64) -> CountResult<usize> {
        let series_list = self.get_series_list()?;
        let mut cleaned_blocks = 0;
        
        for series_name in series_list {
            cleaned_blocks += self.cleanup_series_old_data(&series_name, cutoff_time)?;
        }
        
        Ok(cleaned_blocks)
    }

    fn cleanup_series_old_data(&self, series_name: &str, cutoff_time: u64) -> CountResult<usize> {
        let mut manifest = self.load_series_manifest(series_name)?;
        let mut cleaned_blocks = 0;
        let mut remaining_blocks = Vec::new();
        
        {
            let mut objects = self.objects.lock().unwrap();
            
            for block_meta in manifest.blocks {
                if block_meta.end_time < cutoff_time {
                    objects.remove(&block_meta.key);
                    cleaned_blocks += 1;
                } else {
                    remaining_blocks.push(block_meta);
                }
            }
        }
        
        manifest.blocks = remaining_blocks;
        self.save_series_manifest(&manifest)?;
        
        Ok(cleaned_blocks)
    }

    pub fn migrate_from_disk(&self, _disk_storage: &crate::storage::disk::DiskStorage, _cutoff_time: u64) -> CountResult<usize> {
        // This is a simplified version for testing
        // In real implementation, this would read from disk and archive to S3
        Ok(0)
    }

    // Helper methods for testing
    pub fn object_count(&self) -> usize {
        let objects = self.objects.lock().unwrap();
        objects.len()
    }

    pub fn has_object(&self, key: &str) -> bool {
        let objects = self.objects.lock().unwrap();
        objects.contains_key(key)
    }

    pub fn clear(&self) {
        let mut objects = self.objects.lock().unwrap();
        objects.clear();
    }
}