use count::{CountDB, CountConfig, DataPoint, SeriesKey};
use count::query::AggregationType;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_basic_insert_and_query() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 1000,
        flush_interval_seconds: 60,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let series = SeriesKey::from("test.basic");
    
    // Insert test data
    let base_time = get_current_timestamp();
    let test_points = vec![
        DataPoint::new(base_time, 10.0),
        DataPoint::new(base_time + 1000, 20.0),
        DataPoint::new(base_time + 2000, 30.0),
        DataPoint::new(base_time + 3000, 40.0),
        DataPoint::new(base_time + 4000, 50.0),
    ];
    
    for point in &test_points {
        db.insert(series.clone(), point.clone()).await.unwrap();
    }
    
    // Query back the data
    let results = db.query_range(series, base_time, base_time + 5000).await.unwrap();
    
    assert_eq!(results.len(), 5);
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.timestamp, test_points[i].timestamp);
        assert_eq!(result.value, test_points[i].value);
    }
    
    db.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_aggregation_queries() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 1000,
        flush_interval_seconds: 60,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let series = SeriesKey::from("test.aggregation");
    
    // Insert known test data: 10, 20, 30, 40, 50
    let base_time = get_current_timestamp();
    let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    
    for (i, &value) in values.iter().enumerate() {
        let point = DataPoint::new(base_time + i as u64 * 1000, value);
        db.insert(series.clone(), point).await.unwrap();
    }
    
    // Test all aggregation types
    let sum = db.query_aggregated(series.clone(), base_time, base_time + 5000, AggregationType::Sum).await.unwrap();
    assert_eq!(sum, 150.0, "Sum should be 150.0");
    
    let mean = db.query_aggregated(series.clone(), base_time, base_time + 5000, AggregationType::Mean).await.unwrap();
    assert_eq!(mean, 30.0, "Mean should be 30.0");
    
    let min = db.query_aggregated(series.clone(), base_time, base_time + 5000, AggregationType::Min).await.unwrap();
    assert_eq!(min, 10.0, "Min should be 10.0");
    
    let max = db.query_aggregated(series.clone(), base_time, base_time + 5000, AggregationType::Max).await.unwrap();
    assert_eq!(max, 50.0, "Max should be 50.0");
    
    let count = db.query_aggregated(series, base_time, base_time + 5000, AggregationType::Count).await.unwrap();
    assert_eq!(count, 5.0, "Count should be 5.0");
    
    db.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_memory_buffer_compression() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 50, // Small buffer to trigger compression
        flush_interval_seconds: 60,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let series = SeriesKey::from("test.compression");
    
    // Insert more data than the buffer can hold
    let base_time = get_current_timestamp();
    for i in 0..100 {
        let point = DataPoint::new(base_time + i * 1000, i as f64);
        db.insert(series.clone(), point).await.unwrap();
    }
    
    // Should still be able to query all data
    let results = db.query_range(series, base_time, base_time + 100000).await.unwrap();
    assert_eq!(results.len(), 100, "Should retrieve all 100 points despite compression");
    
    // Verify data integrity
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.timestamp, base_time + i as u64 * 1000);
        assert_eq!(result.value, i as f64);
    }
    
    db.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_disk_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 100,
        flush_interval_seconds: 1, // Quick flush for testing
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let series = SeriesKey::from("test.persistence");
    let base_time = get_current_timestamp();
    
    // Insert data and wait for flush
    {
        let db = CountDB::new(config.clone()).await.unwrap();
        
        for i in 0..150 {
            let point = DataPoint::new(base_time + i * 1000, i as f64);
            db.insert(series.clone(), point).await.unwrap();
        }
        
        // Wait for flush to disk
        sleep(Duration::from_secs(2)).await;
        db.shutdown().await.unwrap();
    }
    
    // Create new database instance and verify data persists
    {
        let db = CountDB::new(config).await.unwrap();
        let results = db.query_range(series, base_time, base_time + 150000).await.unwrap();
        
        // Should be able to retrieve data from disk
        assert!(!results.is_empty(), "Should retrieve persisted data from disk");
        
        db.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn test_multiple_series() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 1000,
        flush_interval_seconds: 60,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let base_time = get_current_timestamp();
    
    let series_names = vec!["cpu.usage", "memory.usage", "disk.io"];
    
    // Insert data into multiple series
    for (series_idx, series_name) in series_names.iter().enumerate() {
        let series = SeriesKey::from(*series_name);
        
        for i in 0..50 {
            let value = (series_idx + 1) as f64 * 100.0 + i as f64;
            let point = DataPoint::new(base_time + i * 1000, value);
            db.insert(series.clone(), point).await.unwrap();
        }
    }
    
    // Query each series and verify independence
    for (series_idx, series_name) in series_names.iter().enumerate() {
        let series = SeriesKey::from(*series_name);
        let results = db.query_range(series, base_time, base_time + 50000).await.unwrap();
        
        assert_eq!(results.len(), 50, "Each series should have 50 points");
        
        // Verify values are correct for this series
        for (i, result) in results.iter().enumerate() {
            let expected_value = (series_idx + 1) as f64 * 100.0 + i as f64;
            assert_eq!(result.value, expected_value, 
                      "Value mismatch in series {} at index {}", series_name, i);
        }
    }
    
    db.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_time_range_queries() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 1000,
        flush_interval_seconds: 60,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let series = SeriesKey::from("test.time_range");
    let base_time = get_current_timestamp();
    
    // Insert data over 10 seconds (10 points)
    for i in 0..10 {
        let point = DataPoint::new(base_time + i * 1000, i as f64);
        db.insert(series.clone(), point).await.unwrap();
    }
    
    // Test various time range queries
    let test_cases = vec![
        (base_time, base_time + 2999, 3, "First 3 seconds"),
        (base_time + 3000, base_time + 6999, 4, "Seconds 3-6"),
        (base_time + 7000, base_time + 9999, 3, "Last 3 seconds"),
        (base_time, base_time + 10000, 10, "All data"),
        (base_time - 5000, base_time - 1000, 0, "Before data range"),
        (base_time + 15000, base_time + 20000, 0, "After data range"),
    ];
    
    for (start_time, end_time, expected_count, description) in test_cases {
        let results = db.query_range(series.clone(), start_time, end_time).await.unwrap();
        assert_eq!(results.len(), expected_count, 
                  "Wrong number of results for {}: expected {}, got {}", 
                  description, expected_count, results.len());
        
        // Verify all results are within the requested time range
        for result in &results {
            assert!(result.timestamp >= start_time && result.timestamp <= end_time,
                   "Point timestamp {} not in range {}..{}", 
                   result.timestamp, start_time, end_time);
        }
    }
    
    db.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_compression_algorithm_correctness() {
    // Test the compression algorithms directly
    use count::compression::{TimestampCompressor, ValueCompressor};
    
    // Test timestamp compression with regular intervals
    let mut ts_compressor = TimestampCompressor::new();
    let timestamps = vec![1000, 1060, 1120, 1180, 1240]; // 60-second intervals
    let mut compressed_sizes = Vec::new();
    
    for &ts in &timestamps {
        let compressed = ts_compressor.compress(ts).unwrap();
        compressed_sizes.push(compressed.len());
    }
    
    // First timestamp should be full size, subsequent should be compressed
    assert_eq!(compressed_sizes[0], 8, "First timestamp should be 8 bytes");
    assert!(compressed_sizes[1] > 1, "Second timestamp should be > 1 byte (delta)");
    assert_eq!(compressed_sizes[2], 1, "Regular interval should compress to 1 byte");
    assert_eq!(compressed_sizes[3], 1, "Regular interval should compress to 1 byte");
    assert_eq!(compressed_sizes[4], 1, "Regular interval should compress to 1 byte");
    
    // Test value compression with identical values
    let mut val_compressor = ValueCompressor::new();
    let values = vec![42.5, 42.5, 42.5, 42.5];
    let mut value_compressed_sizes = Vec::new();
    
    for &val in &values {
        let compressed = val_compressor.compress(val).unwrap();
        value_compressed_sizes.push(compressed.len());
    }
    
    assert_eq!(value_compressed_sizes[0], 8, "First value should be 8 bytes");
    assert_eq!(value_compressed_sizes[1], 1, "Identical value should compress to 1 byte");
    assert_eq!(value_compressed_sizes[2], 1, "Identical value should compress to 1 byte");
    assert_eq!(value_compressed_sizes[3], 1, "Identical value should compress to 1 byte");
}

#[tokio::test]
async fn test_high_throughput_insert() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        memory_buffer_size: 10000,
        flush_interval_seconds: 60,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        cluster_enabled: false,
        node_id: None,
        bind_address: None,
        seed_nodes: Vec::new(),
        replication_factor: 2,
    };
    
    let db = CountDB::new(config).await.unwrap();
    let series = SeriesKey::from("test.throughput");
    let base_time = get_current_timestamp();
    
    let start_insert = std::time::Instant::now();
    
    // Insert 5000 points as fast as possible
    for i in 0..5000 {
        let point = DataPoint::new(base_time + i * 1000, i as f64);
        db.insert(series.clone(), point).await.unwrap();
    }
    
    let insert_duration = start_insert.elapsed();
    let throughput = 5000.0 / insert_duration.as_secs_f64();
    
    println!("Insert throughput: {:.0} points/second", throughput);
    assert!(throughput > 1000.0, "Should achieve > 1000 points/second throughput");
    
    // Verify all data was inserted correctly
    let start_query = std::time::Instant::now();
    let results = db.query_range(series, base_time, base_time + 5000000).await.unwrap();
    let query_duration = start_query.elapsed();
    
    assert_eq!(results.len(), 5000, "Should retrieve all 5000 points");
    println!("Query time for 5000 points: {:?}", query_duration);
    
    db.shutdown().await.unwrap();
}

fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}