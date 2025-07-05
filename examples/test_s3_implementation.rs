use count::storage::mock_s3::MockS3Storage;
use count::storage::memory::CompressedBlock;
use std::collections::HashMap;

fn main() {
    println!("Testing S3 Implementation...");
    
    // Test basic mock S3 operations
    test_basic_operations();
    test_multiple_blocks();
    test_series_list();
    test_cleanup();
    test_range_queries();
    
    println!("All S3 tests passed! ✅");
}

fn test_basic_operations() {
    println!("Testing basic operations...");
    
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
    
    println!("✅ Basic operations test passed");
}

fn test_multiple_blocks() {
    println!("Testing multiple blocks...");
    
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
    
    println!("✅ Multiple blocks test passed");
}

fn test_series_list() {
    println!("Testing series list...");
    
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
    
    println!("✅ Series list test passed");
}

fn test_cleanup() {
    println!("Testing cleanup...");
    
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
    
    println!("✅ Cleanup test passed");
}

fn test_range_queries() {
    println!("Testing range queries...");
    
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
    
    println!("✅ Range queries test passed");
}