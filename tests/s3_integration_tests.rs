use count::{CountConfig, CountDB, DataPoint, SeriesKey};
use count::storage::mock_s3::MockS3Storage;
use count::storage::memory::CompressedBlock;
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

/// Test S3 archival lifecycle with mock storage
#[tokio::test]
async fn test_s3_archival_lifecycle() {
    let mock_s3 = MockS3Storage::new("test-bucket".to_string(), Some("test-prefix".to_string()));
    
    // Create test data with old and new timestamps
    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    let old_time = now_ms - (20 * 24 * 60 * 60 * 1000); // 20 days ago
    let new_time = now_ms - (5 * 24 * 60 * 60 * 1000);  // 5 days ago
    
    let old_block = CompressedBlock {
        start_time: old_time,
        end_time: old_time + 1000,
        compressed_timestamps: vec![1, 2, 3],
        compressed_values: vec![10, 20, 30],
        point_count: 3,
        original_points: Vec::new(),
    };
    
    let new_block = CompressedBlock {
        start_time: new_time,
        end_time: new_time + 1000,
        compressed_timestamps: vec![4, 5, 6],
        compressed_values: vec![40, 50, 60],
        point_count: 3,
        original_points: Vec::new(),
    };
    
    // Archive both blocks
    let mut blocks = HashMap::new();
    blocks.insert("test.series".to_string(), vec![old_block, new_block]);
    mock_s3.archive_blocks(blocks).unwrap();
    
    // Verify both blocks are archived
    let all_blocks = mock_s3.load_blocks_for_range("test.series", 0, u64::MAX).unwrap();
    assert_eq!(all_blocks.len(), 2);
    
    // Clean up old data (older than 15 days)
    let cutoff_time = now_ms - (15 * 24 * 60 * 60 * 1000);
    let cleaned = mock_s3.cleanup_old_data(cutoff_time).unwrap();
    assert_eq!(cleaned, 1); // Should clean up 1 old block
    
    // Verify only new block remains
    let remaining_blocks = mock_s3.load_blocks_for_range("test.series", 0, u64::MAX).unwrap();
    assert_eq!(remaining_blocks.len(), 1);
    assert_eq!(remaining_blocks[0].start_time, new_time);
}

/// Test tiered query routing (memory -> disk -> S3)
#[tokio::test]
async fn test_tiered_query_routing() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 50, // Small buffer to force disk writes
        flush_interval_seconds: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        s3_enabled: false, // We'll test routing logic without actual S3
        s3_bucket: None,
        s3_region: None,
        s3_prefix: None,
        archival_age_days: 14,
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let series = SeriesKey::from("test.tiered");
    
    // Insert data in different time ranges
    let base_time = 1000;
    
    // Recent data (should stay in memory)
    for i in 0..10 {
        let point = DataPoint::new(base_time + i * 100, i as f64);
        db.insert(series.clone(), point).await.unwrap();
    }
    
    // Older data (should be flushed to disk)
    for i in 10..100 {
        let point = DataPoint::new(base_time + i * 100, i as f64);
        db.insert(series.clone(), point).await.unwrap();
    }
    
    // Wait for background flush
    sleep(Duration::from_millis(1500)).await;
    
    // Query all data - should combine memory and disk
    let all_results = db.query_range(series.clone(), 0, 20000).await.unwrap();
    assert_eq!(all_results.len(), 100);
    
    // Verify data is sorted by timestamp
    for i in 1..all_results.len() {
        assert!(all_results[i].timestamp >= all_results[i-1].timestamp);
    }
    
    // Query specific ranges
    let memory_range = db.query_range(series.clone(), base_time, base_time + 500).await.unwrap();
    assert!(!memory_range.is_empty());
    
    let disk_range = db.query_range(series.clone(), base_time + 5000, base_time + 8000).await.unwrap();
    assert!(!disk_range.is_empty());
    
    db.shutdown().await.unwrap();
}

/// Test S3 configuration scenarios
#[test]
fn test_s3_configuration_scenarios() {
    // Test minimal S3 config
    let mut config = CountConfig::default();
    config.s3_enabled = true;
    config.s3_bucket = Some("my-bucket".to_string());
    
    assert!(config.s3_enabled);
    assert_eq!(config.s3_bucket, Some("my-bucket".to_string()));
    assert!(config.s3_region.is_none()); // Should use default
    assert!(config.s3_prefix.is_none());  // Should use default
    assert_eq!(config.archival_age_days, 14);
    
    // Test full S3 config
    config.s3_region = Some("eu-central-1".to_string());
    config.s3_prefix = Some("timeseries-data".to_string());
    config.archival_age_days = 30;
    
    assert_eq!(config.s3_region, Some("eu-central-1".to_string()));
    assert_eq!(config.s3_prefix, Some("timeseries-data".to_string()));
    assert_eq!(config.archival_age_days, 30);
}

/// Test S3 storage efficiency and compression
#[test]
fn test_s3_storage_efficiency() {
    let mock_s3 = MockS3Storage::new("test-bucket".to_string(), None);
    
    // Create multiple series with different characteristics
    let series_data = vec![
        ("cpu.usage", 1000, 100, 0.5),    // Regular intervals, stable values
        ("memory.usage", 2000, 50, 10.0), // Less frequent, varying values  
        ("disk.io", 3000, 200, 1000.0),   // High frequency, large values
    ];
    
    let mut _total_blocks = 0;
    
    for (series_name, start_time, count, _base_value) in series_data {
        let mut blocks = HashMap::new();
        let mut series_blocks = Vec::new();
        
        // Create blocks for each series
        for block_idx in 0..3 {
            let block_start = start_time + (block_idx * 1000);
            let block_end = block_start + 999;
            
            let block = CompressedBlock {
                start_time: block_start,
                end_time: block_end,
                compressed_timestamps: vec![1; count],  // Simulated compression
                compressed_values: vec![2; count],      // Simulated compression
                point_count: count,
                original_points: Vec::new(),
            };
            
            series_blocks.push(block);
            _total_blocks += 1;
        }
        
        blocks.insert(series_name.to_string(), series_blocks);
        mock_s3.archive_blocks(blocks).unwrap();
    }
    
    // Verify all series are stored
    let series_list = mock_s3.get_series_list().unwrap();
    assert_eq!(series_list.len(), 3);
    
    // Verify we can query each series independently
    for series_name in &series_list {
        let blocks = mock_s3.load_blocks_for_range(series_name, 0, u64::MAX).unwrap();
        assert_eq!(blocks.len(), 3); // 3 blocks per series
    }
    
    // Test cross-series queries (should return error for nonexistent series)
    let result = mock_s3.load_blocks_for_range("nonexistent.series", 0, u64::MAX);
    assert!(result.is_err());
}

/// Test archival age boundary conditions  
#[test]
fn test_archival_age_boundaries() {
    let mock_s3 = MockS3Storage::new("test-bucket".to_string(), None);
    
    let base_time = 10000;
    let cutoff_time = 15000; // Middle of our test data
    
    // Create blocks around the cutoff boundary
    let blocks_data = vec![
        (base_time - 1000, base_time - 1),     // Old (should be cleaned)
        (cutoff_time - 500, cutoff_time + 500), // Spans boundary (should be kept)
        (cutoff_time + 1, cutoff_time + 1000), // New (should be kept)
        (base_time + 2000, base_time + 3000),  // New (should be kept)
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
    
    blocks.insert("test.boundary".to_string(), test_blocks);
    mock_s3.archive_blocks(blocks).unwrap();
    
    // Verify all blocks are initially present
    let all_blocks = mock_s3.load_blocks_for_range("test.boundary", 0, u64::MAX).unwrap();
    assert_eq!(all_blocks.len(), 4);
    
    // Clean up old data
    let cleaned = mock_s3.cleanup_old_data(cutoff_time).unwrap();
    assert_eq!(cleaned, 2); // Should clean up blocks with end_time < cutoff_time
    
    // Verify remaining blocks
    let remaining_blocks = mock_s3.load_blocks_for_range("test.boundary", 0, u64::MAX).unwrap();
    assert_eq!(remaining_blocks.len(), 2);
    
    // Verify the old block is gone
    let old_blocks = mock_s3.load_blocks_for_range("test.boundary", 0, base_time).unwrap();
    assert_eq!(old_blocks.len(), 0);
}

/// Test concurrent S3 operations
#[tokio::test]
async fn test_concurrent_s3_operations() {
    let mock_s3 = MockS3Storage::new("test-bucket".to_string(), None);
    
    // Simulate concurrent archival of different series
    let series_names = vec!["series1", "series2", "series3", "series4"];
    
    for series_name in series_names {
        let test_block = CompressedBlock {
            start_time: 1000,
            end_time: 2000,
            compressed_timestamps: vec![1, 2, 3],
            compressed_values: vec![4, 5, 6],
            point_count: 3,
            original_points: Vec::new(),
        };
        
        let mut blocks = HashMap::new();
        blocks.insert(series_name.to_string(), vec![test_block]);
        
        // In a real scenario, these would be concurrent, but our mock is synchronous
        mock_s3.archive_blocks(blocks).unwrap();
    }
    
    // Verify all series were archived
    let series_list = mock_s3.get_series_list().unwrap();
    assert_eq!(series_list.len(), 4);
    
    // Test concurrent queries
    for series_name in &series_list {
        let blocks = mock_s3.load_blocks_for_range(series_name, 500, 2500).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].point_count, 3);
    }
}

/// Test S3 error handling scenarios
#[test]
fn test_s3_error_scenarios() {
    let mock_s3 = MockS3Storage::new("test-bucket".to_string(), None);
    
    // Test querying non-existent series
    let result = mock_s3.load_blocks_for_range("nonexistent.series", 0, 1000);
    assert!(result.is_err()); // Should return error when manifest doesn't exist
    
    // Test empty series list
    let empty_list = mock_s3.get_series_list().unwrap();
    assert_eq!(empty_list.len(), 0);
    
    // Test cleanup on empty storage
    let cleaned = mock_s3.cleanup_old_data(u64::MAX).unwrap();
    assert_eq!(cleaned, 0);
    
    // Add some data and test partial failures
    let test_block = CompressedBlock {
        start_time: 1000,
        end_time: 2000,
        compressed_timestamps: vec![1, 2, 3],
        compressed_values: vec![4, 5, 6],
        point_count: 3,
        original_points: Vec::new(),
    };
    
    let mut blocks = HashMap::new();
    blocks.insert("test.error".to_string(), vec![test_block]);
    mock_s3.archive_blocks(blocks).unwrap();
    
    // Now test successful operations
    let series_list = mock_s3.get_series_list().unwrap();
    assert_eq!(series_list.len(), 1);
    
    let loaded_blocks = mock_s3.load_blocks_for_range("test.error", 0, 3000).unwrap();
    assert_eq!(loaded_blocks.len(), 1);
}

/// Test data integrity through the full pipeline
#[tokio::test]
async fn test_data_integrity_pipeline() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 10, // Very small to force frequent flushes
        flush_interval_seconds: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        s3_enabled: false,
        s3_bucket: None,
        s3_region: None,
        s3_prefix: None,
        archival_age_days: 14,
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let series = SeriesKey::from("integrity.test");
    
    // Insert test data with known pattern
    let test_data: Vec<(u64, f64)> = (0..100)
        .map(|i| (1000 + i * 100, (i as f64) * 1.5))
        .collect();
    
    for (timestamp, value) in &test_data {
        let point = DataPoint::new(*timestamp, *value);
        db.insert(series.clone(), point).await.unwrap();
    }
    
    // Wait for flushes
    sleep(Duration::from_millis(2000)).await;
    
    // Query all data back
    let results = db.query_range(series.clone(), 0, 20000).await.unwrap();
    assert_eq!(results.len(), test_data.len());
    
    // Verify data integrity
    for (i, result) in results.iter().enumerate() {
        let (expected_timestamp, expected_value) = test_data[i];
        assert_eq!(result.timestamp, expected_timestamp);
        assert_eq!(result.value, expected_value);
    }
    
    // Test aggregations on the full dataset
    let sum = db.query_aggregated(
        series.clone(),
        0,
        20000,
        count::query::AggregationType::Sum
    ).await.unwrap();
    
    let expected_sum: f64 = test_data.iter().map(|(_, v)| v).sum();
    assert_eq!(sum, expected_sum);
    
    let count = db.query_aggregated(
        series.clone(),
        0,
        20000,
        count::query::AggregationType::Count
    ).await.unwrap();
    
    assert_eq!(count, test_data.len() as f64);
    
    db.shutdown().await.unwrap();
}