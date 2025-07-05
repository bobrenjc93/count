use crate::{error::{CountError, CountResult}};
use crate::storage::memory::CompressedBlock;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client, config::Region};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct S3Block {
    start_time: u64,
    end_time: u64,
    compressed_timestamps: Vec<u8>,
    compressed_values: Vec<u8>,
    point_count: usize,
}

impl From<CompressedBlock> for S3Block {
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

impl From<S3Block> for CompressedBlock {
    fn from(s3_block: S3Block) -> Self {
        Self {
            start_time: s3_block.start_time,
            end_time: s3_block.end_time,
            compressed_timestamps: s3_block.compressed_timestamps,
            compressed_values: s3_block.compressed_values,
            point_count: s3_block.point_count,
            original_points: Vec::new(), // Don't store original points in S3 to save space
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct S3SeriesManifest {
    series_name: String,
    blocks: Vec<S3BlockMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
struct S3BlockMetadata {
    key: String,
    start_time: u64,
    end_time: u64,
    point_count: usize,
}

pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Storage {
    pub async fn new(bucket: String, region: Option<String>, prefix: Option<String>) -> CountResult<Self> {
        let config = if let Some(region) = region {
            aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(region))
                .load()
                .await
        } else {
            aws_config::load_defaults(BehaviorVersion::latest()).await
        };
        
        let client = Client::new(&config);
        let prefix = prefix.unwrap_or_else(|| "count-data".to_string());
        
        Ok(Self {
            client,
            bucket,
            prefix,
        })
    }

    pub async fn archive_blocks(&self, series_blocks: HashMap<String, Vec<CompressedBlock>>) -> CountResult<()> {
        for (series_name, blocks) in series_blocks {
            self.archive_series_blocks(&series_name, blocks).await?;
        }
        Ok(())
    }

    async fn archive_series_blocks(&self, series_name: &str, blocks: Vec<CompressedBlock>) -> CountResult<()> {
        let mut manifest = self.load_series_manifest(series_name).await.unwrap_or_else(|_| S3SeriesManifest {
            series_name: series_name.to_string(),
            blocks: Vec::new(),
        });

        for block in blocks {
            let key = format!("{}/{}/block_{}_{}.json", self.prefix, series_name, block.start_time, block.end_time);
            
            let s3_block = S3Block::from(block.clone());
            let serialized = serde_json::to_vec(&s3_block)?;
            
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(serialized.into())
                .content_type("application/json")
                .send()
                .await
                .map_err(|e| CountError::Storage {
                    message: format!("Failed to upload block to S3: {}", e),
                })?;
            
            manifest.blocks.push(S3BlockMetadata {
                key: key.clone(),
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
        
        for block_meta in &manifest.blocks {
            // Check if block overlaps with query range
            if block_meta.end_time >= start_time && block_meta.start_time <= end_time {
                let response = self.client
                    .get_object()
                    .bucket(&self.bucket)
                    .key(&block_meta.key)
                    .send()
                    .await
                    .map_err(|e| CountError::Storage {
                        message: format!("Failed to download block from S3: {}", e),
                    })?;
                
                let data = response.body.collect().await
                    .map_err(|e| CountError::Storage {
                        message: format!("Failed to read S3 response body: {}", e),
                    })?
                    .into_bytes();
                
                let s3_block: S3Block = serde_json::from_slice(&data)?;
                blocks.push(CompressedBlock::from(s3_block));
            }
        }
        
        Ok(blocks)
    }

    async fn load_series_manifest(&self, series_name: &str) -> CountResult<S3SeriesManifest> {
        let manifest_key = format!("{}/{}/manifest.json", self.prefix, series_name);
        
        let response = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&manifest_key)
            .send()
            .await
            .map_err(|e| CountError::Storage {
                message: format!("Manifest not found for series {}: {}", series_name, e),
            })?;
        
        let data = response.body.collect().await
            .map_err(|e| CountError::Storage {
                message: format!("Failed to read manifest from S3: {}", e),
            })?
            .into_bytes();
        
        let manifest: S3SeriesManifest = serde_json::from_slice(&data)?;
        Ok(manifest)
    }

    async fn save_series_manifest(&self, manifest: &S3SeriesManifest) -> CountResult<()> {
        let manifest_key = format!("{}/{}/manifest.json", self.prefix, &manifest.series_name);
        
        let serialized = serde_json::to_vec_pretty(manifest)?;
        
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&manifest_key)
            .body(serialized.into())
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| CountError::Storage {
                message: format!("Failed to upload manifest to S3: {}", e),
            })?;
        
        Ok(())
    }

    pub async fn get_series_list(&self) -> CountResult<Vec<String>> {
        let mut series_list = Vec::new();
        let list_prefix = format!("{}/", self.prefix);
        
        let response = self.client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&list_prefix)
            .delimiter("/")
            .send()
            .await
            .map_err(|e| CountError::Storage {
                message: format!("Failed to list S3 objects: {}", e),
            })?;
        
        if let Some(common_prefixes) = response.common_prefixes {
            for prefix in common_prefixes {
                if let Some(prefix_str) = prefix.prefix {
                    // Extract series name from prefix like "count-data/series_name/"
                    let series_name = prefix_str
                        .strip_prefix(&list_prefix)
                        .and_then(|s| s.strip_suffix("/"))
                        .unwrap_or("")
                        .to_string();
                    
                    if !series_name.is_empty() {
                        series_list.push(series_name);
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
        let mut cleaned_blocks = 0;
        let mut remaining_blocks = Vec::new();
        
        for block_meta in manifest.blocks {
            if block_meta.end_time < cutoff_time {
                // Delete old block from S3
                self.client
                    .delete_object()
                    .bucket(&self.bucket)
                    .key(&block_meta.key)
                    .send()
                    .await
                    .map_err(|e| CountError::Storage {
                        message: format!("Failed to delete block from S3: {}", e),
                    })?;
                
                cleaned_blocks += 1;
            } else {
                remaining_blocks.push(block_meta);
            }
        }
        
        // Update manifest with remaining blocks
        manifest.blocks = remaining_blocks;
        self.save_series_manifest(&manifest).await?;
        
        Ok(cleaned_blocks)
    }

    /// Archive blocks from disk storage to S3 and remove from disk
    pub async fn migrate_from_disk(&self, disk_storage: &crate::storage::disk::DiskStorage, cutoff_time: u64) -> CountResult<usize> {
        let series_list = disk_storage.get_series_list().await?;
        let mut migrated_blocks = 0;
        
        for series_name in series_list {
            let blocks = disk_storage.load_blocks_for_range(&series_name, 0, cutoff_time).await?;
            
            if !blocks.is_empty() {
                // Archive to S3
                let mut series_blocks = HashMap::new();
                series_blocks.insert(series_name.clone(), blocks.clone());
                self.archive_blocks(series_blocks).await?;
                
                migrated_blocks += blocks.len();
            }
        }
        
        // Clean up old data from disk after successful migration
        disk_storage.cleanup_old_data(cutoff_time).await?;
        
        Ok(migrated_blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{memory::CompressedBlock, mock_s3::MockS3Storage};
    use std::collections::HashMap;

    // Note: These tests require AWS credentials and an S3 bucket to run
    // They are marked as ignored by default
    
    #[tokio::test]
    #[ignore]
    async fn test_s3_storage_basic_operations() {
        let bucket = std::env::var("TEST_S3_BUCKET").expect("TEST_S3_BUCKET env var required");
        let storage = S3Storage::new(bucket, None, Some("test-count-data".to_string())).await.unwrap();
        
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
        
        // Archive to S3
        storage.archive_blocks(blocks).await.unwrap();
        
        // Load back from S3
        let loaded_blocks = storage.load_blocks_for_range("test.series", 500, 2500).await.unwrap();
        assert_eq!(loaded_blocks.len(), 1);
        assert_eq!(loaded_blocks[0].start_time, 1000);
        assert_eq!(loaded_blocks[0].point_count, 3);
        
        // Cleanup
        storage.cleanup_old_data(u64::MAX).await.unwrap();
    }

    // Mock S3 tests that don't require AWS credentials
    
    #[test]
    fn test_mock_s3_basic_operations() {
        let storage = MockS3Storage::new("test-bucket".to_string(), Some("test-prefix".to_string()));
        
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
        blocks.insert("test.series".to_string(), vec![test_block.clone()]);
        
        // Archive to mock S3
        storage.archive_blocks(blocks).unwrap();
        
        // Verify objects were created
        assert!(storage.object_count() >= 2); // At least manifest + block
        assert!(storage.has_object("test-prefix/test.series/manifest.json"));
        assert!(storage.has_object("test-prefix/test.series/block_1000_2000.json"));
        
        // Load back from mock S3
        let loaded_blocks = storage.load_blocks_for_range("test.series", 500, 2500).unwrap();
        assert_eq!(loaded_blocks.len(), 1);
        assert_eq!(loaded_blocks[0].start_time, 1000);
        assert_eq!(loaded_blocks[0].end_time, 2000);
        assert_eq!(loaded_blocks[0].point_count, 3);
    }

    #[test]
    fn test_mock_s3_multiple_blocks() {
        let storage = MockS3Storage::new("test-bucket".to_string(), None);
        
        // Create multiple test blocks
        let mut blocks = HashMap::new();
        let block1 = CompressedBlock {
            start_time: 1000,
            end_time: 2000,
            compressed_timestamps: vec![1, 2, 3],
            compressed_values: vec![4, 5, 6],
            point_count: 3,
            original_points: Vec::new(),
        };
        let block2 = CompressedBlock {
            start_time: 2000,
            end_time: 3000,
            compressed_timestamps: vec![7, 8, 9],
            compressed_values: vec![10, 11, 12],
            point_count: 3,
            original_points: Vec::new(),
        };
        blocks.insert("test.series".to_string(), vec![block1, block2]);
        
        storage.archive_blocks(blocks).unwrap();
        
        // Test range queries
        let all_blocks = storage.load_blocks_for_range("test.series", 0, 4000).unwrap();
        assert_eq!(all_blocks.len(), 2);
        
        let partial_blocks = storage.load_blocks_for_range("test.series", 1500, 2500).unwrap();
        assert_eq!(partial_blocks.len(), 2); // Both blocks overlap with range
        
        let single_block = storage.load_blocks_for_range("test.series", 500, 1500).unwrap();
        assert_eq!(single_block.len(), 1);
        assert_eq!(single_block[0].start_time, 1000);
    }

    #[test]
    fn test_mock_s3_series_list() {
        let storage = MockS3Storage::new("test-bucket".to_string(), None);
        
        // Create test data for multiple series
        let mut blocks1 = HashMap::new();
        let test_block = CompressedBlock {
            start_time: 1000,
            end_time: 2000,
            compressed_timestamps: vec![1, 2, 3],
            compressed_values: vec![4, 5, 6],
            point_count: 3,
            original_points: Vec::new(),
        };
        blocks1.insert("series1".to_string(), vec![test_block.clone()]);
        
        let mut blocks2 = HashMap::new();
        blocks2.insert("series2".to_string(), vec![test_block]);
        
        storage.archive_blocks(blocks1).unwrap();
        storage.archive_blocks(blocks2).unwrap();
        
        let series_list = storage.get_series_list().unwrap();
        assert_eq!(series_list.len(), 2);
        assert!(series_list.contains(&"series1".to_string()));
        assert!(series_list.contains(&"series2".to_string()));
    }

    #[test]
    fn test_mock_s3_cleanup_old_data() {
        let storage = MockS3Storage::new("test-bucket".to_string(), None);
        
        // Create blocks with different timestamps
        let old_block = CompressedBlock {
            start_time: 1000,
            end_time: 2000,
            compressed_timestamps: vec![1, 2, 3],
            compressed_values: vec![4, 5, 6],
            point_count: 3,
            original_points: Vec::new(),
        };
        let new_block = CompressedBlock {
            start_time: 5000,
            end_time: 6000,
            compressed_timestamps: vec![7, 8, 9],
            compressed_values: vec![10, 11, 12],
            point_count: 3,
            original_points: Vec::new(),
        };
        
        let mut blocks = HashMap::new();
        blocks.insert("test.series".to_string(), vec![old_block, new_block]);
        storage.archive_blocks(blocks).unwrap();
        
        // Clean up data older than timestamp 4000
        let cleaned = storage.cleanup_old_data(4000).unwrap();
        assert_eq!(cleaned, 1); // Should remove 1 old block
        
        // Verify only new block remains
        let remaining_blocks = storage.load_blocks_for_range("test.series", 0, 10000).unwrap();
        assert_eq!(remaining_blocks.len(), 1);
        assert_eq!(remaining_blocks[0].start_time, 5000);
    }

    #[test]
    fn test_mock_s3_range_queries() {
        let storage = MockS3Storage::new("test-bucket".to_string(), None);
        
        // Create blocks with different time ranges
        let blocks_data = vec![
            (1000, 2000),
            (2000, 3000), 
            (3000, 4000),
            (4000, 5000),
        ];
        
        let mut blocks = HashMap::new();
        let test_blocks: Vec<CompressedBlock> = blocks_data.into_iter().map(|(start, end)| {
            CompressedBlock {
                start_time: start,
                end_time: end,
                compressed_timestamps: vec![1, 2, 3],
                compressed_values: vec![4, 5, 6],
                point_count: 3,
                original_points: Vec::new(),
            }
        }).collect();
        
        blocks.insert("test.series".to_string(), test_blocks);
        storage.archive_blocks(blocks).unwrap();
        
        // Test different range queries
        let all_blocks = storage.load_blocks_for_range("test.series", 0, 6000).unwrap();
        assert_eq!(all_blocks.len(), 4);
        
        let middle_blocks = storage.load_blocks_for_range("test.series", 1500, 3500).unwrap();
        assert_eq!(middle_blocks.len(), 3); // Blocks 1000-2000, 2000-3000, 3000-4000
        
        let no_blocks = storage.load_blocks_for_range("test.series", 6000, 7000).unwrap();
        assert_eq!(no_blocks.len(), 0);
        
        let edge_case = storage.load_blocks_for_range("test.series", 2000, 2000).unwrap();
        assert!(edge_case.len() >= 1); // Should include blocks that touch this timestamp
    }

    #[test] 
    fn test_s3_block_conversion() {
        let original_block = CompressedBlock {
            start_time: 1000,
            end_time: 2000,
            compressed_timestamps: vec![1, 2, 3, 4, 5],
            compressed_values: vec![10, 20, 30, 40, 50],
            point_count: 5,
            original_points: vec![], // S3 doesn't store original points
        };
        
        // Convert to S3 format and back
        let s3_block = S3Block::from(original_block.clone());
        let converted_block = CompressedBlock::from(s3_block);
        
        // Verify data integrity (except original_points which is intentionally empty)
        assert_eq!(converted_block.start_time, original_block.start_time);
        assert_eq!(converted_block.end_time, original_block.end_time);
        assert_eq!(converted_block.compressed_timestamps, original_block.compressed_timestamps);
        assert_eq!(converted_block.compressed_values, original_block.compressed_values);
        assert_eq!(converted_block.point_count, original_block.point_count);
        assert!(converted_block.original_points.is_empty()); // S3 doesn't store original points
    }
}