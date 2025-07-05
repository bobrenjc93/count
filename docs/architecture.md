# Architecture Overview

This document provides a comprehensive overview of Count's architecture, from high-level system design to detailed component interactions.

## High-Level Architecture

Count is designed as a distributed, masterless time series database with the following key architectural principles:

- **Shared-Nothing Architecture**: Each node is independent with its own storage
- **Consistent Hashing**: Automatic data distribution without central coordination
- **Master-less Replication**: No single point of failure
- **Eventual Consistency**: High availability with tunable consistency levels

## System Components

### Core Database Layer

#### Storage Engine (`src/storage/`)
The storage engine provides the foundation for time series data management:

```
┌─────────────────┐
│  Storage Engine │
├─────────────────┤
│ Memory Buffer   │ ← Write buffering and batching
│ Disk Storage    │ ← Persistent storage with compression
│ Query Processor │ ← Range queries and aggregations
└─────────────────┘
```

**Components:**
- **Memory Buffer**: Configurable in-memory write buffer for batching
- **Disk Storage**: Compressed time series blocks on disk
- **Query Engine**: Optimized range scans and aggregation operations

**Key Features:**
- Write-optimized with configurable flush intervals
- Time-series specific compression algorithms
- Efficient range queries with timestamp indexing

#### Data Model
```rust
// Core data structures
struct DataPoint {
    timestamp: u64,    // Unix timestamp in milliseconds
    value: f64,        // Metric value
}

struct SeriesKey(String);  // Time series identifier
```

### Clustering Layer (`src/cluster/`)

#### Consistent Hash Ring (`src/cluster/sharding.rs`)
Implements consistent hashing for data distribution:

```
Hash Ring (SHA-256):
   Node 1 (150 virtual nodes)
      ↓
   0x0000...
   0x1111...  ← Virtual nodes distributed around ring
   0x2222...
      ...
   Node 2 (150 virtual nodes)
      ↓
   0x5555...
   0x6666...
      ...
   Node 3 (150 virtual nodes)
      ↓
   0xAAAA...
   0xBBBB...
      ...
   0xFFFF...
```

**Algorithm:**
1. Hash series key with SHA-256
2. Find first virtual node ≥ hash value
3. Map virtual node to physical node
4. For replication, continue clockwise for N nodes

**Benefits:**
- Minimal data movement when nodes join/leave
- Even distribution with virtual nodes
- Predictable data placement

#### Node Discovery (`src/cluster/discovery.rs`)
Gossip-based cluster membership management:

```
Gossip Protocol Flow:
Node A ──→ Node B: Heartbeat + Node List
Node B ──→ Node C: Updated Node List  
Node C ──→ Node A: Node Status Updates
```

**Message Types:**
- `Heartbeat`: Regular node health indicators
- `NodeJoin`: New node joining cluster
- `NodeLeave`: Graceful node departure
- `NodeFailed`: Failure detection notification
- `ClusterState`: Full cluster membership info

**Failure Detection:**
- Configurable heartbeat intervals (default: 5s)
- Failure timeout detection (default: 30s)
- Automatic marking of unreachable nodes
- Gossip-based failure propagation

#### Replication Service (`src/cluster/replication.rs`)
Multi-master replication with configurable consistency:

```
Write Path:
Client ──→ Node A (Coordinator)
           │
           ├──→ Node B (Replica 1)
           └──→ Node C (Replica 2)
           
Read Path:
Client ──→ Any Node ──→ Query all replicas
                    └──→ Merge and return results
```

**Consistency Levels:**
- **Write**: Majority acknowledgment required
- **Read**: Query all replicas, merge results
- **Repair**: Background inconsistency detection and repair

### Distributed Query Layer (`src/distributed/`)

#### Query Router (`src/distributed/query_router.rs`)
Handles distributed query processing:

```
Query Processing Flow:
1. Client sends query to any node
2. Determine which nodes have relevant data
3. Parallel query execution across nodes
4. Result aggregation and deduplication
5. Return merged results to client
```

**Query Types:**
- **Range Queries**: Return raw data points in time range
- **Aggregated Queries**: Compute aggregations (sum, mean, min, max, count)
- **Multi-Series Queries**: Query multiple time series simultaneously

#### Cluster Coordinator (`src/distributed/coordinator.rs`)
Orchestrates all cluster operations:

```
Coordinator Responsibilities:
├── Node lifecycle management
├── Data sharding decisions  
├── Replication coordination
├── Query routing
└── Cluster state management
```

## Data Flow

### Write Path
```
1. Client ──POST /insert──→ Node A
2. Node A determines replicas using hash ring
3. Node A writes locally (if replica)
4. Node A replicates to other nodes in parallel
5. Node A waits for majority acknowledgment
6. Node A responds to client
```

### Read Path
```
1. Client ──GET /query──→ Node A
2. Node A determines data location using hash ring
3. Node A queries relevant nodes in parallel
4. Nodes return their local results
5. Node A merges and deduplicates results
6. Node A returns final result to client
```

## Network Communication

### HTTP API Layer (`src/bin/server.rs`)
RESTful HTTP API for all operations:

```
External API:
├── POST /insert           ← Data ingestion
├── GET /query/range/*     ← Range queries
├── GET /query/aggregated/* ← Aggregation queries
└── GET /health            ← Health monitoring

Internal API:
├── POST /cluster/join     ← Node joining
├── POST /cluster/gossip   ← Gossip messages
├── POST /replicate        ← Data replication
└── POST /query/*          ← Internal queries
```

### Request Flow
```
Load Balancer
     │
     ▼
┌─────────┐  ┌─────────┐  ┌─────────┐
│ Node 1  │  │ Node 2  │  │ Node 3  │
│ (HTTP)  │  │ (HTTP)  │  │ (HTTP)  │
└─────────┘  └─────────┘  └─────────┘
     │            │            │
     └────────────┼────────────┘
                  │
            Internal Cluster
            Communication
```

## Storage Architecture

### Memory Management
```
Memory Buffer Structure:
┌─────────────────────────────────┐
│          Memory Buffer          │
├─────────────────────────────────┤
│ Series A: [Point1, Point2, ...] │
│ Series B: [Point1, Point2, ...] │
│ Series C: [Point1, Point2, ...] │
└─────────────────────────────────┘
                │
                ▼ (Flush Trigger)
┌─────────────────────────────────┐
│           Disk Storage          │
├─────────────────────────────────┤
│  Compressed Time Series Blocks  │
└─────────────────────────────────┘
```

### Disk Layout
```
Data Directory Structure:
/data/
├── series_a/
│   ├── 1640995200000.block    ← Time-based blocks
│   ├── 1640998800000.block
│   └── index.meta             ← Series metadata
├── series_b/
│   ├── 1640995200000.block
│   └── index.meta
└── cluster.state              ← Cluster membership
```

## Fault Tolerance

### Node Failure Scenarios

#### Single Node Failure
```
Before: [Node1] [Node2] [Node3]
         ↓       ↓       ↓
        Data    Data    Data
        
After:  [Node1] [FAILED] [Node3]
         ↓                ↓
        Data             Data
        
Result: Queries automatically route to remaining replicas
```

#### Network Partition
```
Partition:  [Node1] | [Node2] [Node3]
Strategy:   Accept writes in majority partition
           Reject writes in minority partition
           Heal automatically when partition resolves
```

#### Data Consistency
- **Write Conflicts**: Last-write-wins with timestamp ordering
- **Read Repair**: Detect and fix inconsistencies during queries
- **Anti-Entropy**: Background process to synchronize replicas

## Performance Characteristics

### Scalability
- **Linear Write Scaling**: Each node handles 1/N of writes
- **Linear Read Scaling**: Queries distributed across nodes
- **Storage Scaling**: Total capacity = N × node capacity

### Latency Profile
```
Operation           Latency (p99)
─────────────────   ─────────────
Single Write        < 10ms
Range Query         < 50ms
Aggregated Query    < 100ms
Node Discovery      < 5s
```

### Throughput Benchmarks
```
Configuration: 3 nodes, 2x replication
Write Throughput: ~300K points/second
Read Throughput:  ~150K queries/second
Storage Density:  ~2-4 bytes per point
```

## Configuration Architecture

### Hierarchical Configuration
```
Configuration Sources (priority order):
1. Environment Variables    ← Docker/Kubernetes
2. Configuration Files      ← Local deployment
3. Command Line Arguments   ← Development
4. Default Values          ← Fallback
```

### Key Parameters
- **Node Identity**: `NODE_ID`, `BIND_ADDRESS`
- **Cluster Topology**: `CLUSTER_NODES`, `REPLICATION_FACTOR`
- **Performance Tuning**: `MEMORY_BUFFER_SIZE`, `FLUSH_INTERVAL`
- **Operational**: `DATA_DIR`, `LOG_LEVEL`

## Security Considerations

### Network Security
- HTTP-only communication (HTTPS termination at load balancer)
- No authentication built-in (rely on network security)
- Cluster communication on private networks only

### Data Security
- No encryption at rest (delegate to filesystem/volume encryption)
- No data masking or anonymization
- Audit logging for operational events

## Monitoring and Observability

### Health Endpoints
```
GET /health - Node and cluster health status
Response: {
  "status": "healthy",
  "node_id": 1,
  "cluster_size": 3
}
```

### Metrics Collection
- Node-level metrics: CPU, memory, disk usage
- Database metrics: Write rate, query latency, storage size
- Cluster metrics: Node count, replication health, partition status

### Logging
- Structured JSON logging with tracing
- Configurable log levels (ERROR, WARN, INFO, DEBUG, TRACE)
- Request tracing with correlation IDs

## Future Architecture Considerations

### Planned Enhancements
- **Multi-Region Support**: Cross-datacenter replication
- **Advanced Indexing**: Secondary indexes for metadata queries
- **Stream Processing**: Real-time aggregation and alerting
- **SQL Interface**: SQL-like query language for analytics

### Scalability Roadmap
- **Horizontal Partitioning**: Split large time series across nodes
- **Tiered Storage**: Hot/warm/cold data lifecycle management  
- **Query Optimization**: Cost-based query planning
- **Caching Layer**: Distributed query result caching