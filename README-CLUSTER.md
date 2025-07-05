# Count - Distributed Time Series Database

A high-performance, distributed time series database with multi-node sharding and replication support.

## Features

### Core Database Features
- High-performance time series data storage
- Memory buffering with configurable flush intervals
- Disk-based persistence with compression
- Range queries and aggregations (sum, mean, min, max, count)

### Distributed Features
- **Consistent Hashing**: Automatic data sharding across cluster nodes
- **Replication**: Configurable replication factor for fault tolerance
- **Node Discovery**: Gossip-based cluster membership management
- **Distributed Queries**: Automatic query routing and result aggregation
- **Failure Detection**: Automatic detection and handling of node failures

## Architecture

### Sharding Strategy
- Uses consistent hashing with virtual nodes (150 virtual nodes per physical node)
- SHA-256 based hash function for even data distribution
- Automatic data rebalancing when nodes join/leave

### Replication
- Master-less replication with configurable replication factor
- Write operations replicated to N nodes (default: 2)
- Read operations can query any replica for better performance
- Consistency level: eventual consistency with repair mechanisms

### Node Communication
- HTTP-based inter-node communication
- Gossip protocol for cluster membership
- Background heartbeats and failure detection
- Automatic cluster state synchronization

## Quick Start with Docker Compose

### 1. Start the Cluster

```bash
# Build and start a 3-node cluster
docker-compose up -d

# Check cluster health
curl http://localhost:8081/health
curl http://localhost:8082/health  
curl http://localhost:8083/health
```

### 2. Insert Data

```bash
# Insert data points (will be automatically sharded)
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "cpu.usage.total", "timestamp": 1640995200000, "value": 75.5}'

curl -X POST http://localhost:8082/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "memory.usage.percent", "timestamp": 1640995200000, "value": 60.2}'
```

### 3. Query Data

```bash
# Range query (distributed across relevant nodes)
curl "http://localhost:8081/query/range/cpu.usage.total?start=1640995000000&end=1640995300000"

# Aggregated query (distributed computation)
curl "http://localhost:8082/query/aggregated/cpu.usage.total?start=1640995000000&end=1640995300000&aggregation=mean"
```

## Manual Deployment

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NODE_ID` | Unique node identifier (required) | - |
| `CLUSTER_ENABLED` | Enable cluster mode | `false` |
| `BIND_ADDRESS` | Server bind address | `127.0.0.1:8080` |
| `CLUSTER_NODES` | Comma-separated list of cluster nodes | - |
| `REPLICATION_FACTOR` | Number of replicas per data point | `2` |
| `DATA_DIR` | Data storage directory | `./count_data` |
| `RUST_LOG` | Log level | `info` |

### Single Node Deployment

```bash
# Build the application
cargo build --release

# Start single node
export NODE_ID=1
export BIND_ADDRESS=0.0.0.0:8080
export DATA_DIR=/data/count
./target/release/count-server
```

### Multi-Node Cluster

```bash
# Node 1 (seed node)
export NODE_ID=1
export CLUSTER_ENABLED=true
export BIND_ADDRESS=0.0.0.0:8080
export CLUSTER_NODES=node1:8080,node2:8080,node3:8080
export REPLICATION_FACTOR=2
./target/release/count-server

# Node 2
export NODE_ID=2
export CLUSTER_ENABLED=true
export BIND_ADDRESS=0.0.0.0:8080
export CLUSTER_NODES=node1:8080,node2:8080,node3:8080
export REPLICATION_FACTOR=2
./target/release/count-server

# Node 3
export NODE_ID=3
export CLUSTER_ENABLED=true
export BIND_ADDRESS=0.0.0.0:8080
export CLUSTER_NODES=node1:8080,node2:8080,node3:8080
export REPLICATION_FACTOR=2
./target/release/count-server
```

## API Reference

### Health Check
```http
GET /health
```

### Insert Data
```http
POST /insert
Content-Type: application/json

{
  "series": "metric.name",
  "timestamp": 1640995200000,
  "value": 42.0
}
```

### Range Query
```http
GET /query/range/{series}?start={timestamp}&end={timestamp}
```

### Aggregated Query
```http
GET /query/aggregated/{series}?start={timestamp}&end={timestamp}&aggregation={type}
```

Aggregation types: `sum`, `mean`, `min`, `max`, `count`

## Monitoring and Operations

### Cluster Status
```bash
# Check individual node health
curl http://localhost:8081/health

# View cluster information in logs
docker-compose logs count-node-1
```

### Scaling Operations

#### Adding a Node
1. Update `docker-compose.yml` to add new node
2. Restart cluster: `docker-compose up -d`
3. New node will automatically join and receive data

#### Removing a Node
1. Stop the node: `docker-compose stop count-node-X`
2. Update configuration to remove from cluster
3. Data will be automatically re-replicated

### Backup and Recovery
- Data is stored in Docker volumes: `count-data-1`, `count-data-2`, `count-data-3`
- Backup volumes regularly for disaster recovery
- Multiple replicas provide automatic failure recovery

## Performance Characteristics

### Write Performance
- ~100K+ writes/second per node (depends on hardware)
- Linear scaling with additional nodes
- Configurable memory buffering for batch writes

### Read Performance  
- ~50K+ range queries/second per node
- Distributed queries scale with cluster size
- Automatic query optimization and caching

### Storage Efficiency
- Time-series optimized compression
- ~2-4 bytes per data point (typical)
- Automatic data retention and cleanup

## Troubleshooting

### Common Issues

1. **Node can't join cluster**
   - Check network connectivity between nodes
   - Verify `CLUSTER_NODES` configuration
   - Check firewall settings on port 8080

2. **Data inconsistency**
   - Check replication factor vs cluster size
   - Monitor node health and network partitions
   - Use read repair mechanisms

3. **High memory usage**
   - Adjust `memory_buffer_size` configuration
   - Reduce `flush_interval_seconds` for more frequent flushes
   - Monitor per-node data distribution

### Debug Logging
```bash
# Enable debug logging
export RUST_LOG=debug
./target/release/count-server

# Or with Docker Compose
RUST_LOG=debug docker-compose up
```

## Development

### Running Tests
```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration_tests

# Benchmarks
cargo bench
```

### Architecture Overview
```
┌─────────────┐   ┌─────────────┐   ┌─────────────┐
│   Node 1    │   │   Node 2    │   │   Node 3    │
│  (Primary)  │   │ (Replica 1) │   │ (Replica 2) │
├─────────────┤   ├─────────────┤   ├─────────────┤
│   HTTP API  │   │   HTTP API  │   │   HTTP API  │
│  Discovery  │   │  Discovery  │   │  Discovery  │
│ Replication │   │ Replication │   │ Replication │
│   Storage   │   │   Storage   │   │   Storage   │
└─────────────┘   └─────────────┘   └─────────────┘
       │                 │                 │
       └─────────────────┼─────────────────┘
                         │
              ┌─────────────────┐
              │  Consistent     │
              │  Hash Ring      │
              │   (Sharding)    │
              └─────────────────┘
```

The application implements a distributed hash table architecture with:
- Consistent hashing for data distribution
- Gossip protocol for node discovery
- Multi-master replication for fault tolerance
- HTTP-based query routing and aggregation