# Clustering Guide

This guide covers everything you need to know about setting up and managing multi-node Count clusters, from basic concepts to advanced operations.

## Clustering Concepts

### Core Principles

Count uses a **masterless, shared-nothing architecture** with these key characteristics:

- **No Single Point of Failure**: Every node can accept reads and writes
- **Automatic Sharding**: Data automatically distributed using consistent hashing
- **Configurable Replication**: Data replicated to multiple nodes for fault tolerance
- **Gossip-Based Discovery**: Nodes discover each other automatically
- **Eventual Consistency**: High availability with tunable consistency levels

### Consistent Hashing

Count uses consistent hashing to distribute data evenly across nodes:

```
Hash Ring Visualization:
     0x0000
       |
   Node 3 ---- Node 1
       |        |
     0xAAAA    0x3333
       |        |
   Node 2 ---- Hash Values
     0x7777
```

**How it works:**
1. Each series name is hashed (SHA-256)
2. Hash value maps to position on ring
3. Data stored on next N nodes clockwise (N = replication factor)
4. Virtual nodes (150 per physical node) ensure even distribution

### Replication Strategy

**Write Path:**
```
Client Write → Coordinator Node → Replicas (parallel)
                    ↓
               Wait for majority ACK
                    ↓
               Respond to client
```

**Read Path:**
```
Client Read → Any Node → Query all replicas → Merge results
```

**Consistency Levels:**
- **Write Quorum**: Majority of replicas must acknowledge
- **Read Repair**: Inconsistencies detected and fixed during reads
- **Anti-Entropy**: Background sync process for replicas

## Setting Up a Cluster

### Docker Compose Deployment (Recommended)

The easiest way to set up a cluster:

```yaml
# docker-compose.yml
version: '3.8'

services:
  count-node-1:
    build: .
    container_name: count-node-1
    environment:
      - NODE_ID=1
      - CLUSTER_ENABLED=true
      - CLUSTER_NODES=count-node-1:8080,count-node-2:8080,count-node-3:8080
      - REPLICATION_FACTOR=2
      - BIND_ADDRESS=0.0.0.0:8080
      - DATA_DIR=/data
      - RUST_LOG=info
    ports:
      - "8081:8080"
    volumes:
      - count-data-1:/data
    networks:
      - count-cluster

  count-node-2:
    build: .
    container_name: count-node-2
    environment:
      - NODE_ID=2
      - CLUSTER_ENABLED=true
      - CLUSTER_NODES=count-node-1:8080,count-node-2:8080,count-node-3:8080
      - REPLICATION_FACTOR=2
      - BIND_ADDRESS=0.0.0.0:8080
      - DATA_DIR=/data
      - RUST_LOG=info
    ports:
      - "8082:8080"
    volumes:
      - count-data-2:/data
    networks:
      - count-cluster
    depends_on:
      - count-node-1

  count-node-3:
    build: .
    container_name: count-node-3
    environment:
      - NODE_ID=3
      - CLUSTER_ENABLED=true
      - CLUSTER_NODES=count-node-1:8080,count-node-2:8080,count-node-3:8080
      - REPLICATION_FACTOR=2
      - BIND_ADDRESS=0.0.0.0:8080
      - DATA_DIR=/data
      - RUST_LOG=info
    ports:
      - "8083:8080"
    volumes:
      - count-data-3:/data
    networks:
      - count-cluster
    depends_on:
      - count-node-1
      - count-node-2

volumes:
  count-data-1:
  count-data-2:
  count-data-3:

networks:
  count-cluster:
    driver: bridge
```

**Start the cluster:**
```bash
docker-compose up -d
```

### Manual Multi-Node Setup

For bare metal or custom deployments:

**Node 1 (Seed Node):**
```bash
export NODE_ID=1
export CLUSTER_ENABLED=true
export BIND_ADDRESS=0.0.0.0:8080
export CLUSTER_NODES=192.168.1.10:8080,192.168.1.11:8080,192.168.1.12:8080
export REPLICATION_FACTOR=2
export DATA_DIR=/opt/count/data
./count-server
```

**Node 2:**
```bash
export NODE_ID=2
export CLUSTER_ENABLED=true
export BIND_ADDRESS=0.0.0.0:8080
export CLUSTER_NODES=192.168.1.10:8080,192.168.1.11:8080,192.168.1.12:8080
export REPLICATION_FACTOR=2
export DATA_DIR=/opt/count/data
./count-server
```

**Node 3:**
```bash
export NODE_ID=3
export CLUSTER_ENABLED=true
export BIND_ADDRESS=0.0.0.0:8080
export CLUSTER_NODES=192.168.1.10:8080,192.168.1.11:8080,192.168.1.12:8080
export REPLICATION_FACTOR=2
export DATA_DIR=/opt/count/data
./count-server
```

### Kubernetes Deployment

Example StatefulSet for Kubernetes:

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: count-cluster
spec:
  serviceName: count-cluster
  replicas: 3
  selector:
    matchLabels:
      app: count
  template:
    metadata:
      labels:
        app: count
    spec:
      containers:
      - name: count
        image: count:latest
        ports:
        - containerPort: 8080
        env:
        - name: NODE_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: CLUSTER_ENABLED
          value: "true"
        - name: BIND_ADDRESS
          value: "0.0.0.0:8080"
        - name: CLUSTER_NODES
          value: "count-cluster-0.count-cluster:8080,count-cluster-1.count-cluster:8080,count-cluster-2.count-cluster:8080"
        - name: REPLICATION_FACTOR
          value: "2"
        - name: DATA_DIR
          value: "/data"
        volumeMounts:
        - name: data
          mountPath: /data
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 10Gi
---
apiVersion: v1
kind: Service
metadata:
  name: count-cluster
spec:
  clusterIP: None
  selector:
    app: count
  ports:
  - port: 8080
```

## Cluster Operations

### Verifying Cluster Health

**Check individual nodes:**
```bash
for port in 8081 8082 8083; do
  echo "=== Node on port $port ==="
  curl -s http://localhost:$port/health | jq
  echo
done
```

**Expected healthy output:**
```json
{
  "status": "healthy",
  "node_id": 1,
  "cluster_size": 3
}
```

### Understanding Data Distribution

**Test data distribution:**
```bash
# Insert data with different series names
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "app1.cpu", "timestamp": 1640995200000, "value": 45.0}'

curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "app2.cpu", "timestamp": 1640995200000, "value": 67.0}'

curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "app3.cpu", "timestamp": 1640995200000, "value": 23.0}'

# Query from different nodes - should work from any node
curl "http://localhost:8082/query/range/app1.cpu?start=1640995000000&end=1640996000000"
curl "http://localhost:8083/query/range/app2.cpu?start=1640995000000&end=1640996000000"
```

### Load Balancing Reads and Writes

You can send requests to any node:

```bash
# Round-robin example
nodes=("http://localhost:8081" "http://localhost:8082" "http://localhost:8083")

for i in {1..9}; do
  node=${nodes[$((i % 3))]}
  curl -X POST $node/insert \
    -H "Content-Type: application/json" \
    -d "{\"series\": \"test.series\", \"timestamp\": $((1640995200000 + i * 1000)), \"value\": $((RANDOM % 100))}"
  echo "Inserted to $node"
done
```

## Scaling Operations

### Adding Nodes

**1. Prepare the new node:**
```bash
# Node 4 configuration
export NODE_ID=4
export CLUSTER_ENABLED=true
export BIND_ADDRESS=0.0.0.0:8080
export CLUSTER_NODES=node1:8080,node2:8080,node3:8080,node4:8080
export REPLICATION_FACTOR=2
```

**2. Update existing nodes:**
Update `CLUSTER_NODES` on all existing nodes to include the new node, then restart them.

**3. Start the new node:**
```bash
./count-server
```

**4. Verify cluster membership:**
```bash
curl http://localhost:8081/health  # Should show cluster_size: 4
```

**Note:** Currently, adding nodes requires updating all node configurations and restarts. Future versions will support dynamic node addition.

### Removing Nodes

**Graceful removal:**

1. **Stop accepting new writes** to the node (implement at load balancer level)
2. **Wait for data to replicate** to other nodes
3. **Update cluster configuration** on remaining nodes
4. **Restart remaining nodes** with updated `CLUSTER_NODES`
5. **Stop the removed node**

**Handling node failures:**
If a node fails unexpectedly, the cluster will automatically:
- Detect the failure via gossip protocol
- Mark the node as unreachable
- Continue serving requests from remaining replicas
- Attempt to heal when the node comes back online

### Cluster Rebalancing

When nodes are added or removed, data needs to be rebalanced:

**Current Status:** Manual rebalancing required
**Future Enhancement:** Automatic rebalancing

**Manual rebalancing process:**
1. Identify data that needs to move
2. Stream data to new nodes
3. Verify data integrity
4. Remove data from old locations

## Failure Scenarios and Handling

### Single Node Failure

**Scenario:** One node goes down
**Impact:** No data loss (with replication factor ≥ 2)
**Recovery:** Automatic - remaining nodes serve requests

```bash
# Simulate node failure
docker-compose stop count-node-2

# Verify cluster still works
curl http://localhost:8081/health  # Should show cluster_size: 2

# Insert and query data - should still work
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "test.failure", "timestamp": 1640995200000, "value": 42.0}'

# Restart failed node
docker-compose start count-node-2

# Verify recovery
curl http://localhost:8082/health  # Should be back online
```

### Network Partition

**Scenario:** Cluster splits into two groups

**Behavior:**
- **Majority partition:** Continues accepting writes
- **Minority partition:** Rejects writes, serves stale reads
- **Automatic healing:** When partition resolves, data syncs

**Testing network partition:**
```bash
# Block network between nodes (requires iptables/firewall rules)
# This is environment-specific and should be tested carefully

# Example with Docker networks:
docker network disconnect count_count-cluster count-node-3
# Node 3 is now isolated

# Verify behavior:
# - Nodes 1,2 should continue working (majority)
# - Node 3 should reject writes
```

### Complete Cluster Failure

**Recovery procedure:**

1. **Start one node** with existing data
2. **Let it form single-node cluster**
3. **Add other nodes one by one**
4. **Wait for data synchronization**

```bash
# Recovery example
docker-compose up count-node-1  # Start first node
# Verify it's working

docker-compose up count-node-2  # Add second node
# Wait for join

docker-compose up count-node-3  # Add third node
# Verify full cluster recovery
```

## Performance Tuning for Clusters

### Replication Factor Selection

**Guidelines:**
- **RF=1:** No fault tolerance, maximum performance
- **RF=2:** Survives 1 node failure, good balance
- **RF=3:** Survives 2 node failures, higher durability cost

**Performance impact:**
```
Replication Factor | Write Latency | Storage Overhead
1                 | 1x           | 1x
2                 | 1.5x         | 2x  
3                 | 2x           | 3x
```

### Node Sizing

**Recommended node specifications:**

**Small Deployment (Dev/Test):**
- 2 CPU cores
- 4GB RAM
- 50GB SSD
- Handles ~10K writes/sec

**Medium Deployment (Production):**
- 4 CPU cores
- 8GB RAM
- 200GB SSD
- Handles ~50K writes/sec

**Large Deployment (High Scale):**
- 8+ CPU cores
- 16+ GB RAM
- 500GB+ NVMe SSD
- Handles ~100K+ writes/sec

### Network Considerations

**Bandwidth requirements:**
- **Inter-node replication:** ~2x write bandwidth
- **Query distribution:** Variable based on query patterns
- **Gossip protocol:** Minimal (<1Mbps per node)

**Latency recommendations:**
- **Same datacenter:** <1ms preferred
- **Cross-AZ:** <10ms acceptable
- **Cross-region:** >50ms not recommended

### Memory Configuration

**Buffer sizing:**
```bash
# Large memory buffer = fewer disk writes, more memory usage
export MEMORY_BUFFER_SIZE=50000     # 50K points in memory

# Small memory buffer = more frequent disk writes, less memory
export MEMORY_BUFFER_SIZE=5000      # 5K points in memory

# Flush interval affects memory usage and durability
export FLUSH_INTERVAL_SECONDS=60    # More frequent flushing
export FLUSH_INTERVAL_SECONDS=600   # Less frequent flushing
```

## Monitoring Clusters

### Health Monitoring

**Automated health check script:**
```bash
#!/bin/bash
NODES=("localhost:8081" "localhost:8082" "localhost:8083")
FAILED_NODES=()

for node in "${NODES[@]}"; do
  response=$(curl -s -w "%{http_code}" http://$node/health -o /dev/null)
  if [ "$response" != "200" ]; then
    FAILED_NODES+=($node)
  fi
done

if [ ${#FAILED_NODES[@]} -eq 0 ]; then
  echo "All nodes healthy"
  exit 0
else
  echo "Failed nodes: ${FAILED_NODES[*]}"
  exit 1
fi
```

### Metrics to Monitor

**Per-Node Metrics:**
- HTTP response times
- Memory usage
- Disk usage and I/O
- CPU utilization
- Network I/O

**Cluster Metrics:**
- Total node count
- Healthy node count
- Replication health
- Query distribution
- Data balance

### Log Analysis

**Important log patterns to watch:**
```bash
# Node joining/leaving
grep "Node.*joining\|leaving" /var/log/count/*.log

# Replication failures
grep "Replication.*failed" /var/log/count/*.log

# Query routing issues
grep "Query.*failed\|timeout" /var/log/count/*.log

# Network partition detection
grep "Network partition\|Split brain" /var/log/count/*.log
```

## Troubleshooting Clusters

### Common Issues

**1. Nodes not joining cluster:**
```bash
# Check network connectivity
telnet other-node 8080

# Check configuration
env | grep CLUSTER

# Check logs
docker-compose logs count-node-1 | grep -i "join\|discover"
```

**2. Data inconsistency:**
```bash
# Query same series from different nodes
curl "http://localhost:8081/query/range/test.series?start=0&end=9999999999999"
curl "http://localhost:8082/query/range/test.series?start=0&end=9999999999999"

# Compare results - should be identical
```

**3. Write failures:**
```bash
# Check replication status
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "debug.write", "timestamp": 1640995200000, "value": 1.0}' \
  -v  # Verbose output shows response details
```

**4. High latency:**
```bash
# Check if nodes are overloaded
curl -w "@curl-format.txt" -s http://localhost:8081/health

# curl-format.txt:
#     time_namelookup:  %{time_namelookup}\n
#        time_connect:  %{time_connect}\n
#     time_appconnect:  %{time_appconnect}\n
#    time_pretransfer:  %{time_pretransfer}\n
#       time_redirect:  %{time_redirect}\n
#  time_starttransfer:  %{time_starttransfer}\n
#                     ----------\n
#          time_total:  %{time_total}\n
```

### Advanced Debugging

**Enable debug logging:**
```bash
# Increase log verbosity
export RUST_LOG=debug

# Or for specific modules
export RUST_LOG=count::cluster=debug,count::distributed=debug
```

**Network debugging:**
```bash
# Monitor inter-node traffic
tcpdump -i any port 8080

# Check network latency between nodes
ping other-node
traceroute other-node
```

This clustering guide provides the foundation for running Count in production. For specific deployment scenarios, see the [Deployment Guide](deployment.md) and [Operations Guide](operations.md).