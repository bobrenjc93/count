# Development Guide

This guide covers setting up a development environment, understanding the codebase, contributing to Count, and extending its functionality.

## Development Environment Setup

### Prerequisites

- **Rust 1.75+** with cargo
- **Docker & Docker Compose** for testing
- **Git** for version control
- **curl** and **jq** for API testing

### Quick Setup

```bash
# Clone the repository
git clone <repository-url>
cd count

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install development dependencies
cargo install cargo-watch cargo-audit cargo-deny

# Build the project
cargo build

# Run tests
cargo test

# Start development server
cargo run --bin count-server
```

### Development Dependencies

Add these to your local development environment:

```bash
# Code formatting and linting
rustup component add rustfmt clippy

# Additional development tools
cargo install cargo-expand    # Macro expansion
cargo install cargo-tree      # Dependency tree
cargo install cargo-outdated  # Check for outdated dependencies
cargo install flamegraph      # Performance profiling
```

### IDE Setup

#### VS Code

Recommended extensions:
- rust-analyzer
- CodeLLDB (for debugging)
- Better TOML
- Docker

**VS Code settings (`.vscode/settings.json`):**
```json
{
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.features": "all",
    "files.watcherExclude": {
        "**/target/**": true
    }
}
```

**Launch configuration (`.vscode/launch.json`):**
```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug Count Server",
            "cargo": {
                "args": ["build", "--bin=count-server"],
                "filter": {
                    "name": "count-server",
                    "kind": "bin"
                }
            },
            "args": [],
            "cwd": "${workspaceFolder}",
            "env": {
                "NODE_ID": "1",
                "BIND_ADDRESS": "127.0.0.1:8080",
                "DATA_DIR": "./dev_data",
                "RUST_LOG": "debug"
            }
        }
    ]
}
```

## Codebase Overview

### Project Structure

```
count/
├── src/
│   ├── lib.rs                 # Main library entry point
│   ├── error.rs               # Error definitions
│   ├── compression/           # Data compression algorithms
│   │   ├── mod.rs
│   │   ├── timestamp.rs       # Timestamp compression
│   │   └── value.rs           # Value compression
│   ├── storage/               # Storage layer
│   │   ├── mod.rs
│   │   ├── engine.rs          # Main storage engine
│   │   ├── memory.rs          # In-memory buffer
│   │   └── disk.rs            # Disk persistence
│   ├── query/                 # Query processing
│   │   └── mod.rs
│   ├── cluster/               # Clustering functionality
│   │   ├── mod.rs
│   │   ├── sharding.rs        # Consistent hashing
│   │   ├── node.rs            # Node representation
│   │   ├── discovery.rs       # Node discovery via gossip
│   │   └── replication.rs     # Data replication
│   ├── distributed/           # Distributed operations
│   │   ├── mod.rs
│   │   ├── query_router.rs    # Distributed query routing
│   │   └── coordinator.rs     # Cluster coordination
│   └── bin/
│       ├── cli.rs             # Command-line interface
│       └── server.rs          # HTTP server
├── tests/
│   └── integration_tests.rs   # Integration tests
├── benches/                   # Benchmarks
│   ├── compression.rs
│   └── query.rs
├── examples/                  # Usage examples
├── docs/                      # Documentation
├── Cargo.toml                 # Project configuration
├── Dockerfile                 # Container build
└── docker-compose.yml         # Multi-node setup
```

### Key Components

#### Storage Layer (`src/storage/`)

**StorageEngine** - The main storage coordinator:
- Manages memory buffer and disk storage
- Handles background flushing
- Coordinates queries across memory and disk

**MemoryBuffer** - In-memory write buffer:
- Buffers writes for batch processing
- Implements time-based and size-based flushing
- Provides efficient in-memory queries

**DiskStorage** - Persistent storage:
- Compressed time-series blocks
- Efficient range queries
- Automatic cleanup and maintenance

#### Clustering Layer (`src/cluster/`)

**ConsistentHashRing** - Data distribution:
- SHA-256 based hashing
- Virtual nodes for even distribution
- Handles node addition/removal

**DiscoveryService** - Node membership:
- Gossip-based protocol
- Failure detection
- Cluster state synchronization

**ReplicationService** - Data replication:
- Multi-master replication
- Consistency levels
- Repair mechanisms

#### Distributed Layer (`src/distributed/`)

**DistributedQueryRouter** - Query distribution:
- Routes queries to appropriate nodes
- Merges results from multiple nodes
- Handles partial failures

**ClusterCoordinator** - Overall coordination:
- Orchestrates all cluster operations
- Manages node lifecycle
- Handles configuration changes

### Data Flow

#### Write Path
```
1. HTTP Request → server.rs
2. ClusterCoordinator.insert() → coordinator.rs
3. Determine replicas → sharding.rs
4. Local write → StorageEngine.insert() → engine.rs
5. Replication → ReplicationService → replication.rs
6. Response to client
```

#### Read Path
```
1. HTTP Request → server.rs  
2. ClusterCoordinator.query_*() → coordinator.rs
3. DistributedQueryRouter.execute_distributed_query() → query_router.rs
4. Parallel queries to nodes
5. Merge results → query_router.rs
6. Response to client
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test modules
cargo test storage
cargo test cluster
cargo test integration

# Run tests with output
cargo test -- --nocapture

# Run tests in single thread (for debugging)
cargo test -- --test-threads=1
```

### Test Categories

**Unit Tests** - Component-level testing:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistent_hash_ring() {
        let ring = ConsistentHashRing::new(150);
        // Test implementation
    }
    
    #[tokio::test]
    async fn test_storage_engine() {
        let config = CountConfig::default();
        let engine = StorageEngine::new(&config).await.unwrap();
        // Test implementation
    }
}
```

**Integration Tests** - End-to-end testing:
```rust
// tests/integration_tests.rs
use count::*;

#[tokio::test]
async fn test_single_node_operations() {
    let config = CountConfig::default();
    let db = CountDB::new(config).await.unwrap();
    
    // Test insert and query
    let series = SeriesKey::from("test.series");
    let point = DataPoint::new(1000, 42.0);
    db.insert(series.clone(), point).await.unwrap();
    
    let results = db.query_range(series, 0, 2000).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].value, 42.0);
}
```

**Benchmark Tests** - Performance testing:
```rust
// benches/compression.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use count::compression::*;

fn benchmark_timestamp_compression(c: &mut Criterion) {
    let timestamps: Vec<u64> = (0..1000).map(|i| i * 1000).collect();
    
    c.bench_function("compress timestamps", |b| {
        b.iter(|| {
            compress_timestamps(black_box(&timestamps))
        })
    });
}

criterion_group!(benches, benchmark_timestamp_compression);
criterion_main!(benches);
```

### Testing Best Practices

**1. Use temporary directories for tests:**
```rust
use tempfile::TempDir;

#[tokio::test]
async fn test_with_temp_storage() {
    let temp_dir = TempDir::new().unwrap();
    let config = CountConfig {
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    
    // Test with isolated storage
}
```

**2. Mock external dependencies:**
```rust
// For testing cluster operations without actual network
#[cfg(test)]
mod mocks {
    use super::*;
    
    pub struct MockDiscoveryService {
        nodes: Vec<Node>,
    }
    
    impl MockDiscoveryService {
        pub fn new() -> Self {
            Self { nodes: Vec::new() }
        }
    }
}
```

**3. Test error conditions:**
```rust
#[tokio::test]
async fn test_insert_with_invalid_data() {
    let db = setup_test_db().await;
    
    let result = db.insert(
        SeriesKey::from(""),  // Invalid empty series
        DataPoint::new(0, 0.0)
    ).await;
    
    assert!(result.is_err());
}
```

## Debugging

### Logging and Tracing

**Enable debug logging:**
```bash
export RUST_LOG=debug
cargo run --bin count-server

# Or module-specific logging
export RUST_LOG=count::cluster=debug,count::storage=info
```

**Add tracing to your code:**
```rust
use tracing::{debug, info, warn, error, instrument};

#[instrument(skip(self))]
pub async fn insert(&self, series: SeriesKey, point: DataPoint) -> CountResult<()> {
    debug!("Inserting point for series: {}", series.0);
    
    // Implementation
    
    info!("Successfully inserted point");
    Ok(())
}
```

### Performance Profiling

**CPU profiling with flamegraph:**
```bash
# Install flamegraph
cargo install flamegraph

# Profile the application
cargo flamegraph --bin count-server

# Profile specific benchmarks
cargo flamegraph --bench compression
```

**Memory profiling with valgrind:**
```bash
# Install valgrind (Linux only)
sudo apt-get install valgrind

# Run with memory checking
cargo build
valgrind --tool=memcheck --leak-check=full ./target/debug/count-server
```

### Debugging Distributed Issues

**Test cluster locally:**
```bash
# Start 3-node cluster
docker-compose up -d

# Monitor logs from all nodes
docker-compose logs -f

# Test specific scenarios
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "debug.test", "timestamp": 1640995200000, "value": 42}'

# Check data distribution
for port in 8081 8082 8083; do
  echo "Node $port:"
  curl -s "http://localhost:$port/query/range/debug.test?start=0&end=9999999999999" | jq
done
```

## Contributing

### Development Workflow

1. **Fork and clone the repository**
2. **Create a feature branch:**
   ```bash
   git checkout -b feature/my-new-feature
   ```

3. **Make changes and test:**
   ```bash
   cargo test
   cargo clippy
   cargo fmt
   ```

4. **Update documentation if needed**
5. **Submit a pull request**

### Code Style

**Follow Rust conventions:**
```rust
// Use meaningful names
pub struct ConsistentHashRing {
    ring: Arc<RwLock<BTreeMap<u64, NodeId>>>,
    virtual_nodes: usize,
}

// Document public APIs
/// Adds a node to the consistent hash ring
/// 
/// # Arguments
/// * `node_id` - Unique identifier for the node
pub async fn add_node(&self, node_id: NodeId) {
    // Implementation
}

// Use proper error handling
pub fn parse_config(input: &str) -> Result<Config, ConfigError> {
    // Implementation
}
```

**Run formatting and linting:**
```bash
# Format code
cargo fmt

# Check for common mistakes
cargo clippy

# Check for security issues
cargo audit
```

### Writing Tests

**Test new features thoroughly:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_new_feature() {
        // Setup
        let config = test_config();
        let system = setup_test_system(config).await;
        
        // Execute
        let result = system.my_new_feature().await;
        
        // Verify
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value.expected_field, expected_value);
    }
    
    #[tokio::test] 
    async fn test_new_feature_error_case() {
        // Test error conditions
    }
}
```

**Add integration tests for user-facing features:**
```rust
// tests/integration_tests.rs
#[tokio::test]
async fn test_new_api_endpoint() {
    let cluster = setup_test_cluster().await;
    
    // Test the new endpoint
    let response = reqwest::post("http://localhost:8080/new-endpoint")
        .json(&test_payload)
        .send()
        .await
        .unwrap();
        
    assert_eq!(response.status(), 200);
}
```

### Documentation

**Update relevant documentation:**
- **Code comments** for complex logic
- **API documentation** for new endpoints
- **README** for user-facing changes
- **Architecture docs** for significant changes

**Example API documentation:**
```rust
/// Insert a data point into the time series database
/// 
/// # Arguments
/// * `series` - Time series identifier (e.g., "cpu.usage.total")
/// * `point` - Data point with timestamp and value
/// 
/// # Returns
/// * `Ok(())` if successful
/// * `Err(CountError)` if insertion fails
/// 
/// # Examples
/// ```
/// let db = CountDB::new(config).await?;
/// let series = SeriesKey::from("cpu.usage");
/// let point = DataPoint::new(1640995200000, 75.5);
/// db.insert(series, point).await?;
/// ```
pub async fn insert(&self, series: SeriesKey, point: DataPoint) -> CountResult<()> {
    // Implementation
}
```

## Extending Count

### Adding New Storage Backends

**1. Define the storage trait:**
```rust
// src/storage/backends/mod.rs
#[async_trait]
pub trait StorageBackend {
    async fn write_block(&self, series: &str, block: &CompressedBlock) -> CountResult<()>;
    async fn read_blocks(&self, series: &str, start: u64, end: u64) -> CountResult<Vec<CompressedBlock>>;
    async fn list_series(&self) -> CountResult<Vec<String>>;
    async fn delete_series(&self, series: &str) -> CountResult<()>;
}
```

**2. Implement for your backend:**
```rust
// src/storage/backends/s3.rs
pub struct S3StorageBackend {
    client: aws_sdk_s3::Client,
    bucket: String,
}

#[async_trait]
impl StorageBackend for S3StorageBackend {
    async fn write_block(&self, series: &str, block: &CompressedBlock) -> CountResult<()> {
        let key = format!("series/{}/blocks/{}.block", series, block.start_time);
        
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(block.data.clone().into())
            .send()
            .await
            .map_err(|e| CountError::StorageError(e.to_string()))?;
            
        Ok(())
    }
    
    // Implement other methods...
}
```

### Adding New Query Types

**1. Extend the AggregationType enum:**
```rust
// src/query/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationType {
    Sum,
    Mean,
    Min,
    Max,
    Count,
    // New aggregation types
    Median,
    StdDev,
    Percentile(f64),
}
```

**2. Implement the aggregation logic:**
```rust
pub fn compute_aggregation(points: &[DataPoint], agg_type: AggregationType) -> f64 {
    match agg_type {
        AggregationType::Median => {
            let mut values: Vec<f64> = points.iter().map(|p| p.value).collect();
            values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            let len = values.len();
            if len % 2 == 0 {
                (values[len / 2 - 1] + values[len / 2]) / 2.0
            } else {
                values[len / 2]
            }
        }
        AggregationType::Percentile(p) => {
            let mut values: Vec<f64> = points.iter().map(|p| p.value).collect();
            values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            let index = (p / 100.0 * (values.len() - 1) as f64) as usize;
            values[index]
        }
        // Handle existing types...
        _ => existing_aggregation_logic(points, agg_type),
    }
}
```

### Adding New Network Protocols

**1. Define the protocol trait:**
```rust
// src/cluster/protocols/mod.rs
#[async_trait]
pub trait ClusterProtocol {
    async fn send_message(&self, target: &Node, message: &GossipMessage) -> CountResult<()>;
    async fn broadcast_message(&self, nodes: &[Node], message: &GossipMessage) -> CountResult<()>;
    async fn start_listener(&self, bind_addr: SocketAddr) -> CountResult<()>;
}
```

**2. Implement for your protocol:**
```rust
// src/cluster/protocols/grpc.rs
pub struct GrpcProtocol {
    client: GrpcClient,
}

#[async_trait]
impl ClusterProtocol for GrpcProtocol {
    async fn send_message(&self, target: &Node, message: &GossipMessage) -> CountResult<()> {
        let request = tonic::Request::new(message.clone());
        self.client.send_gossip(request).await
            .map_err(|e| CountError::NetworkError(e.to_string()))?;
        Ok(())
    }
    
    // Implement other methods...
}
```

## Performance Optimization

### Profiling Guidelines

**1. Identify bottlenecks:**
```bash
# Profile CPU usage
cargo flamegraph --bin count-server

# Profile memory allocations
cargo build --release
valgrind --tool=massif ./target/release/count-server
```

**2. Benchmark critical paths:**
```rust
// benches/critical_path.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_insert_path(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let db = rt.block_on(setup_test_db());
    
    c.bench_function("insert data point", |b| {
        b.to_async(&rt).iter(|| async {
            let series = SeriesKey::from("bench.test");
            let point = DataPoint::new(timestamp(), random_value());
            db.insert(series, point).await.unwrap();
        });
    });
}
```

**3. Optimize hot paths:**
```rust
// Use efficient data structures
use dashmap::DashMap;  // Concurrent HashMap
use parking_lot::RwLock;  // Fast RwLock

// Avoid unnecessary allocations
pub fn process_series_name(name: &str) -> &str {
    // Return slice instead of String when possible
    name.trim()
}

// Use memory pools for frequent allocations
use object_pool::Pool;

lazy_static! {
    static ref BUFFER_POOL: Pool<Vec<u8>> = Pool::new(100, Vec::new);
}
```

### Memory Management

**1. Monitor memory usage:**
```rust
use sysinfo::{System, SystemExt};

pub fn log_memory_usage() {
    let mut system = System::new_all();
    system.refresh_all();
    
    info!("Memory usage: {} MB", system.used_memory() / 1024 / 1024);
}
```

**2. Implement memory limits:**
```rust
pub struct MemoryBuffer {
    max_size: usize,
    current_size: AtomicUsize,
    data: DashMap<String, Vec<DataPoint>>,
}

impl MemoryBuffer {
    pub fn insert(&self, series: String, point: DataPoint) -> CountResult<()> {
        let point_size = std::mem::size_of::<DataPoint>();
        
        if self.current_size.load(Ordering::Relaxed) + point_size > self.max_size {
            return Err(CountError::MemoryLimitExceeded);
        }
        
        self.data.entry(series).or_insert_with(Vec::new).push(point);
        self.current_size.fetch_add(point_size, Ordering::Relaxed);
        
        Ok(())
    }
}
```

This development guide provides everything needed to contribute to Count and extend its functionality. The modular architecture makes it straightforward to add new features while maintaining code quality and performance.