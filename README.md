# Gorilla-Inspired Time Series Database

A high-performance, fault-tolerant, in-memory time series database built in Rust, inspired by Facebook's Gorilla paper. Designed for fast ingestion and compression of recent data with support for horizontal sharding, cross-region replication, and pluggable object storage backends.

## 🚀 Features

### Core Capabilities
- **High-Performance Compression**: Delta-of-delta encoding for timestamps and XOR compression for floating-point values
- **Fast Ingestion**: Optimized for high-throughput time series data ingestion
- **Efficient Queries**: Range queries, aggregations, and correlation analysis
- **2-Hour Blocks**: Automatic data organization into compressed 2-hour blocks

### Scalability & Reliability
- **Horizontal Sharding**: Consistent hashing with automatic rebalancing
- **Cross-Region Replication**: Write to multiple regions with failover support
- **Fault Tolerance**: WAL (Write-Ahead Logging) and checkpointing for crash recovery
- **Object Storage Support**: Pluggable backends (Local FS, S3, and custom implementations)

### Advanced Analytics
- **Correlation Analysis**: Pearson and Spearman correlation with rolling windows
- **Aggregation Functions**: Mean, sum, min, max, percentiles, standard deviation
- **Auto-correlation**: Time series pattern analysis
- **Roll-up Jobs**: Background aggregation for coarse-grained data

## 🏗️ Architecture

The database is built with a modular architecture:

```
tsdb-core/          # Main database interface and coordination
├── compression/    # Delta-of-delta and XOR compression algorithms  
├── storage/        # In-memory storage, WAL, checkpointing, object store abstraction
├── query/          # Query engine, aggregations, correlation analysis
├── replication/    # Cross-region replication and failover management
├── shard/          # Consistent hashing and shard management
└── tools/          # Benchmarking and testing utilities
```

## 🛠️ Quick Start

### Installation

```bash
git clone <repository-url>
cd ods
cargo build --release
```

### Basic Usage

```rust
use tsdb_core::{TimeSeriesDatabase, DatabaseConfig, DataPoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create database with default configuration
    let database = TimeSeriesDatabase::new(DatabaseConfig::default()).await?;
    
    // Insert data points
    let timestamp = 1640995200000; // 2022-01-01 00:00:00 UTC
    let point = DataPoint::new(timestamp, 42.0);
    
    database.insert_point("cpu.usage", point).await?;
    
    // Query data
    let points = database.query_range(
        "cpu.usage",
        1640995200000,  // start
        1640995260000   // end (1 minute later)
    ).await?;
    
    println!("Retrieved {} points", points.len());
    
    database.shutdown().await?;
    Ok(())
}
```

### With S3 Backend

```rust
let database = TimeSeriesDatabase::new(DatabaseConfig::default())
    .await?
    .with_s3_store(
        "primary".to_string(),
        "my-timeseries-bucket".to_string(),
        "us-east-1".to_string(),
        None, // Use default S3 endpoint
        "access_key".to_string(),
        "secret_key".to_string(),
    ).await?;
```

## 🧪 Testing & Benchmarks

### Run Tests
```bash
cargo test
```

### Run Benchmarks
```bash
cargo run --bin tools benchmark
```

### Interactive Demo
```bash
cargo run --bin tools demo
```

### Test Compression
```bash
cargo run --bin tools test-compression
```

### Test Correlation Analysis
```bash
cargo run --bin tools test-correlation
```

## 📊 Performance Characteristics

### Compression Ratios
- **Timestamps**: 10-20x compression for regular intervals
- **Values**: 2-8x compression depending on data patterns
- **Overall**: Typically 3-10x compression ratio

### Throughput (typical hardware)
- **Writes**: 100K-500K points/second per shard
- **Reads**: 1M+ points/second per shard
- **Queries**: Sub-millisecond for recent data blocks

### Storage Efficiency
- **Memory**: ~26 hours of recent data in memory
- **Disk/Object Storage**: Compressed historical blocks
- **Network**: Minimal replication overhead with compression

## 🔧 Configuration

### Database Configuration

```rust
use tsdb_core::DatabaseConfig;
use shard::ShardConfig;
use replication::ReplicationConfig;

let config = DatabaseConfig {
    shard_config: ShardConfig {
        shard_count: 256,
        replica_count: 2,
        rebalance_threshold: 0.1,
        ..Default::default()
    },
    replication_config: ReplicationConfig {
        replica_count: 2,
        enable_cross_region: true,
        ..Default::default()
    },
    enable_replication: true,
    enable_sharding: true,
    node_id: "node_1".to_string(),
    region: "us-east-1".to_string(),
    ..Default::default()
};
```

### Storage Configuration

```rust
use storage::PersistentStorageConfig;

let storage_config = PersistentStorageConfig {
    block_duration_hours: 2,
    checkpoint_interval_hours: 6,
    wal_sync_interval_ms: 1000,
    max_wal_size_mb: 100,
    compression_enabled: true,
    object_store_name: Some("s3".to_string()),
};
```

## 🌐 Object Storage Abstraction

The database supports pluggable object storage backends:

### Local File System
```rust
use storage::{LocalFileStore, ObjectStoreManager};

let mut store_manager = ObjectStoreManager::new();
let local_store = Box::new(LocalFileStore::new("./data")?);
store_manager.add_store("local".to_string(), local_store);
```

### S3-Compatible Storage
```rust
use storage::{S3Store, S3Credentials};

let credentials = S3Credentials {
    access_key: "your_access_key".to_string(),
    secret_key: "your_secret_key".to_string(),
    session_token: None,
};

let s3_store = Box::new(S3Store::new(
    "bucket-name".to_string(),
    "us-east-1".to_string(),
    None, // Default endpoint
    credentials,
));
```

### Custom Storage Backend
Implement the `ObjectStore` trait for custom backends:

```rust
use storage::ObjectStore;
use async_trait::async_trait;

struct MyCustomStore;

#[async_trait]
impl ObjectStore for MyCustomStore {
    async fn put_object(&self, key: &str, data: Vec<u8>) -> storage::Result<()> {
        // Your implementation
        Ok(())
    }
    
    async fn get_object(&self, key: &str) -> storage::Result<Vec<u8>> {
        // Your implementation
        Ok(vec![])
    }
    
    // ... implement other required methods
}
```

## 🔍 Query Capabilities

### Basic Queries
```rust
// Range query
let points = database.query_range("series_key", start_time, end_time).await?;

// Get latest point
let latest = database.get_latest_point("series_key").await?;

// Series information
let info = database.get_series_info("series_key").await?;
```

### Advanced Queries
```rust
use query::{QueryRequest, AggregationType};

let request = QueryRequest {
    series_keys: vec!["cpu.usage".to_string()],
    start_time: 1640995200000,
    end_time: 1640998800000,
    aggregation: Some(AggregationType::Mean),
    step_ms: Some(60000), // 1-minute buckets
};

let results = database.query(request).await?;
```

### Correlation Analysis
```rust
use query::CorrelationEngine;

let engine = CorrelationEngine::new();
let correlation = engine.pearson_correlation(&series_a, &series_b)?;
let rolling_corr = engine.rolling_correlation(&series_a, &series_b, 50)?;
```

## 🚦 Operational Features

### Health Monitoring
```rust
let stats = database.get_database_stats().await?;
println!("Total series: {}", stats.total_series);
println!("Compression ratio: {:.2}x", stats.compression_ratio);
```

### Data Lifecycle Management
```rust
// Clean up old data (older than 24 hours)
let cutoff = current_timestamp_ms() - 24 * 3600 * 1000;
let cleaned_points = database.cleanup_old_data(cutoff).await?;
```

### Checkpointing
```rust
// Manual checkpoint creation
let checkpoints = database.create_checkpoint().await?;
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run the test suite: `cargo test`
6. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🙏 Acknowledgments

- Inspired by Facebook's Gorilla time series database
- Built with the Rust ecosystem's excellent crates
- Thanks to the time series database community for insights and best practices# count
