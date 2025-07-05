# Configuration Guide

This document covers all configuration options for Count, including environment variables, configuration files, and deployment-specific settings.

## Configuration Sources

Count uses hierarchical configuration with the following priority (highest to lowest):

1. **Environment Variables** - Runtime configuration
2. **Configuration Files** - Local deployment files (future enhancement)
3. **Command Line Arguments** - Development overrides (future enhancement)
4. **Default Values** - Built-in fallbacks

## Core Configuration

### Node Identity

**NODE_ID**
- **Type:** Integer
- **Required:** Yes (for cluster mode)
- **Description:** Unique identifier for this node in the cluster
- **Example:** `NODE_ID=1`
- **Notes:** Must be unique across all nodes in the cluster

**BIND_ADDRESS**
- **Type:** String (IP:Port)
- **Required:** Yes (for cluster mode)
- **Default:** `127.0.0.1:8080`
- **Description:** Address and port for the HTTP server to bind to
- **Examples:**
  - `BIND_ADDRESS=0.0.0.0:8080` (bind to all interfaces)
  - `BIND_ADDRESS=192.168.1.10:8080` (specific IP)
  - `BIND_ADDRESS=127.0.0.1:9000` (custom port)

## Cluster Configuration

**CLUSTER_ENABLED**
- **Type:** Boolean
- **Default:** `false`
- **Description:** Enable cluster mode
- **Example:** `CLUSTER_ENABLED=true`
- **Notes:** When false, runs in single-node mode

**CLUSTER_NODES**
- **Type:** Comma-separated list
- **Required:** Yes (when clustering enabled)
- **Description:** List of all nodes in the cluster
- **Example:** `CLUSTER_NODES=node1:8080,node2:8080,node3:8080`
- **Format:** `hostname:port` or `ip:port`
- **Notes:**
  - Include all nodes, including this node
  - Used for initial cluster discovery
  - Hostnames must be resolvable by all nodes

**REPLICATION_FACTOR**
- **Type:** Integer
- **Default:** `2`
- **Range:** 1 to cluster size
- **Description:** Number of copies of each data point
- **Examples:**
  - `REPLICATION_FACTOR=1` (no replication)
  - `REPLICATION_FACTOR=2` (one backup copy)
  - `REPLICATION_FACTOR=3` (two backup copies)
- **Guidelines:**
  - RF=1: No fault tolerance, max performance
  - RF=2: Survives 1 node failure
  - RF=3: Survives 2 node failures

## Storage Configuration

**DATA_DIR**
- **Type:** String (file path)
- **Default:** `./count_data`
- **Description:** Directory for persistent data storage
- **Examples:**
  - `DATA_DIR=/var/lib/count`
  - `DATA_DIR=/data/count`
  - `DATA_DIR=./local_data`
- **Notes:**
  - Directory will be created if it doesn't exist
  - Must be writable by the Count process
  - Should be on fast storage (SSD recommended)

**MEMORY_BUFFER_SIZE**
- **Type:** Integer
- **Default:** `10000`
- **Description:** Number of data points to buffer in memory before flushing
- **Examples:**
  - `MEMORY_BUFFER_SIZE=5000` (smaller buffer, more frequent flushes)
  - `MEMORY_BUFFER_SIZE=50000` (larger buffer, less frequent flushes)
- **Guidelines:**
  - Larger values = better write performance, more memory usage
  - Smaller values = lower memory usage, more disk I/O
  - Consider available RAM and write patterns

**FLUSH_INTERVAL_SECONDS**
- **Type:** Integer
- **Default:** `300` (5 minutes)
- **Description:** Maximum time to wait before flushing memory buffer to disk
- **Examples:**
  - `FLUSH_INTERVAL_SECONDS=60` (1 minute, more durable)
  - `FLUSH_INTERVAL_SECONDS=600` (10 minutes, less I/O)
- **Trade-offs:**
  - Lower values = better durability, more disk I/O
  - Higher values = risk more data loss, better performance

## Logging Configuration

**RUST_LOG**
- **Type:** String
- **Default:** `info`
- **Description:** Log level and filtering
- **Examples:**
  - `RUST_LOG=error` (errors only)
  - `RUST_LOG=warn` (warnings and errors)
  - `RUST_LOG=info` (general information)
  - `RUST_LOG=debug` (detailed debugging)
  - `RUST_LOG=trace` (very detailed tracing)
- **Module-specific:** `RUST_LOG=count::cluster=debug,info`

## Performance Tuning

### Memory Configuration

**Recommended memory buffer sizes by workload:**

**High-frequency writes (>1000/sec):**
```bash
export MEMORY_BUFFER_SIZE=50000
export FLUSH_INTERVAL_SECONDS=120
```

**Moderate writes (100-1000/sec):**
```bash
export MEMORY_BUFFER_SIZE=10000
export FLUSH_INTERVAL_SECONDS=300
```

**Low-frequency writes (<100/sec):**
```bash
export MEMORY_BUFFER_SIZE=1000
export FLUSH_INTERVAL_SECONDS=60
```

### Cluster Tuning

**Small cluster (2-3 nodes):**
```bash
export REPLICATION_FACTOR=2
export HEARTBEAT_INTERVAL=5     # Future: gossip heartbeat interval
export FAILURE_TIMEOUT=30       # Future: failure detection timeout
```

**Large cluster (5+ nodes):**
```bash
export REPLICATION_FACTOR=3
export HEARTBEAT_INTERVAL=10    # Future: reduce gossip frequency
export FAILURE_TIMEOUT=60       # Future: longer timeout for large clusters
```

## Environment-Specific Configurations

### Development Environment

```bash
# .env.development
NODE_ID=1
CLUSTER_ENABLED=false
BIND_ADDRESS=127.0.0.1:8080
DATA_DIR=./dev_data
MEMORY_BUFFER_SIZE=1000
FLUSH_INTERVAL_SECONDS=30
RUST_LOG=debug
```

### Testing Environment

```bash
# .env.test
NODE_ID=1
CLUSTER_ENABLED=true
CLUSTER_NODES=test-node-1:8080,test-node-2:8080
REPLICATION_FACTOR=2
BIND_ADDRESS=0.0.0.0:8080
DATA_DIR=/tmp/count_test
MEMORY_BUFFER_SIZE=100
FLUSH_INTERVAL_SECONDS=5
RUST_LOG=info
```

### Production Environment

```bash
# .env.production
NODE_ID=${HOSTNAME##*-}  # Extract number from hostname
CLUSTER_ENABLED=true
CLUSTER_NODES=count-1:8080,count-2:8080,count-3:8080
REPLICATION_FACTOR=2
BIND_ADDRESS=0.0.0.0:8080
DATA_DIR=/var/lib/count
MEMORY_BUFFER_SIZE=25000
FLUSH_INTERVAL_SECONDS=300
RUST_LOG=info
```

## Container Configuration

### Docker Configuration

**Dockerfile environment:**
```dockerfile
ENV NODE_ID=1
ENV CLUSTER_ENABLED=true
ENV BIND_ADDRESS=0.0.0.0:8080
ENV DATA_DIR=/data
ENV MEMORY_BUFFER_SIZE=10000
ENV FLUSH_INTERVAL_SECONDS=300
ENV RUST_LOG=info
```

**Docker run example:**
```bash
docker run -d \
  --name count-node-1 \
  -p 8081:8080 \
  -e NODE_ID=1 \
  -e CLUSTER_ENABLED=true \
  -e CLUSTER_NODES=count-1:8080,count-2:8080,count-3:8080 \
  -e REPLICATION_FACTOR=2 \
  -e BIND_ADDRESS=0.0.0.0:8080 \
  -e DATA_DIR=/data \
  -e RUST_LOG=info \
  -v count-data-1:/data \
  count:latest
```

### Docker Compose Configuration

```yaml
services:
  count-node-1:
    image: count:latest
    environment:
      NODE_ID: 1
      CLUSTER_ENABLED: "true"
      CLUSTER_NODES: "count-node-1:8080,count-node-2:8080,count-node-3:8080"
      REPLICATION_FACTOR: 2
      BIND_ADDRESS: "0.0.0.0:8080"
      DATA_DIR: "/data"
      MEMORY_BUFFER_SIZE: 25000
      FLUSH_INTERVAL_SECONDS: 300
      RUST_LOG: "info"
    volumes:
      - count-data-1:/data
    ports:
      - "8081:8080"
```

### Kubernetes Configuration

**ConfigMap:**
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: count-config
data:
  CLUSTER_ENABLED: "true"
  REPLICATION_FACTOR: "2"
  BIND_ADDRESS: "0.0.0.0:8080"
  DATA_DIR: "/data"
  MEMORY_BUFFER_SIZE: "25000"
  FLUSH_INTERVAL_SECONDS: "300"
  RUST_LOG: "info"
```

**StatefulSet:**
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: count
spec:
  serviceName: count
  replicas: 3
  template:
    spec:
      containers:
      - name: count
        image: count:latest
        env:
        - name: NODE_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: CLUSTER_NODES
          value: "count-0.count:8080,count-1.count:8080,count-2.count:8080"
        envFrom:
        - configMapRef:
            name: count-config
```

## Configuration Validation

### Required Settings Check

Use this script to validate your configuration:

```bash
#!/bin/bash

# validate-config.sh
echo "Validating Count configuration..."

# Check required variables for cluster mode
if [ "$CLUSTER_ENABLED" = "true" ]; then
  if [ -z "$NODE_ID" ]; then
    echo "ERROR: NODE_ID is required when clustering is enabled"
    exit 1
  fi
  
  if [ -z "$BIND_ADDRESS" ]; then
    echo "ERROR: BIND_ADDRESS is required when clustering is enabled"
    exit 1
  fi
  
  if [ -z "$CLUSTER_NODES" ]; then
    echo "ERROR: CLUSTER_NODES is required when clustering is enabled"
    exit 1
  fi
  
  # Validate node ID is numeric
  if ! [[ "$NODE_ID" =~ ^[0-9]+$ ]]; then
    echo "ERROR: NODE_ID must be a positive integer"
    exit 1
  fi
  
  # Validate bind address format
  if ! [[ "$BIND_ADDRESS" =~ ^[0-9.]+:[0-9]+$ ]]; then
    echo "ERROR: BIND_ADDRESS must be in format IP:PORT"
    exit 1
  fi
  
  # Validate replication factor
  node_count=$(echo "$CLUSTER_NODES" | tr ',' '\n' | wc -l)
  if [ "$REPLICATION_FACTOR" -gt "$node_count" ]; then
    echo "ERROR: REPLICATION_FACTOR ($REPLICATION_FACTOR) cannot exceed node count ($node_count)"
    exit 1
  fi
fi

# Check data directory
if [ ! -z "$DATA_DIR" ]; then
  if [ ! -d "$DATA_DIR" ]; then
    echo "WARNING: DATA_DIR ($DATA_DIR) does not exist, will be created"
  elif [ ! -w "$DATA_DIR" ]; then
    echo "ERROR: DATA_DIR ($DATA_DIR) is not writable"
    exit 1
  fi
fi

# Validate numeric settings
if [ ! -z "$MEMORY_BUFFER_SIZE" ] && ! [[ "$MEMORY_BUFFER_SIZE" =~ ^[0-9]+$ ]]; then
  echo "ERROR: MEMORY_BUFFER_SIZE must be a positive integer"
  exit 1
fi

if [ ! -z "$FLUSH_INTERVAL_SECONDS" ] && ! [[ "$FLUSH_INTERVAL_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "ERROR: FLUSH_INTERVAL_SECONDS must be a positive integer"
  exit 1
fi

echo "Configuration validation passed!"
```

### Configuration Examples by Use Case

**High-Throughput Ingestion:**
```bash
export MEMORY_BUFFER_SIZE=100000
export FLUSH_INTERVAL_SECONDS=600
export REPLICATION_FACTOR=1  # Sacrifice durability for performance
export RUST_LOG=warn         # Reduce logging overhead
```

**High-Durability Storage:**
```bash
export MEMORY_BUFFER_SIZE=1000
export FLUSH_INTERVAL_SECONDS=30
export REPLICATION_FACTOR=3
export RUST_LOG=info
```

**Memory-Constrained Environment:**
```bash
export MEMORY_BUFFER_SIZE=1000
export FLUSH_INTERVAL_SECONDS=60
export REPLICATION_FACTOR=2
```

**Development/Testing:**
```bash
export MEMORY_BUFFER_SIZE=100
export FLUSH_INTERVAL_SECONDS=5
export REPLICATION_FACTOR=1
export RUST_LOG=debug
```

## Security Configuration

Currently, Count does not have built-in security features. Security should be implemented at the infrastructure level:

### Network Security

**Firewall rules (iptables example):**
```bash
# Allow only cluster nodes to access Count ports
iptables -A INPUT -p tcp --dport 8080 -s 192.168.1.10 -j ACCEPT
iptables -A INPUT -p tcp --dport 8080 -s 192.168.1.11 -j ACCEPT
iptables -A INPUT -p tcp --dport 8080 -s 192.168.1.12 -j ACCEPT
iptables -A INPUT -p tcp --dport 8080 -j DROP
```

**VPC/Security Groups (AWS example):**
```json
{
  "SecurityGroupRules": [
    {
      "IpProtocol": "tcp",
      "FromPort": 8080,
      "ToPort": 8080,
      "SourceSecurityGroupId": "sg-count-cluster"
    }
  ]
}
```

### TLS/SSL Termination

Use a reverse proxy for HTTPS:

**Nginx configuration:**
```nginx
server {
    listen 443 ssl;
    server_name count.yourdomain.com;
    
    ssl_certificate /etc/ssl/certs/count.crt;
    ssl_certificate_key /etc/ssl/private/count.key;
    
    location / {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

## Monitoring Configuration

### Health Check Configuration

**External health checks:**
```bash
# Add to crontab or monitoring system
*/1 * * * * curl -f http://localhost:8080/health || alert_on_failure
```

**Docker health check:**
```dockerfile
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1
```

### Metrics Export

Currently, Count exports metrics via logs. Future versions will include:
- Prometheus metrics endpoint
- StatsD integration  
- Custom metrics exporters

**Log-based monitoring:**
```bash
# Monitor logs for performance metrics
tail -f /var/log/count/count.log | grep "METRIC:"
```

## Troubleshooting Configuration

### Common Configuration Issues

**1. Node not joining cluster:**
```bash
# Check network connectivity
telnet other-node 8080

# Verify configuration
env | grep CLUSTER
```

**2. High memory usage:**
```bash
# Reduce buffer size
export MEMORY_BUFFER_SIZE=5000
export FLUSH_INTERVAL_SECONDS=120
```

**3. Slow writes:**
```bash
# Increase buffer size
export MEMORY_BUFFER_SIZE=25000
export FLUSH_INTERVAL_SECONDS=600
```

**4. Data loss after restart:**
```bash
# Reduce flush interval
export FLUSH_INTERVAL_SECONDS=60
```

### Configuration Debugging

**Enable debug logging:**
```bash
export RUST_LOG=count=debug
# Or for specific modules:
export RUST_LOG=count::cluster=debug,count::storage=debug
```

**Validate configuration at startup:**
```bash
# Add to startup script
./validate-config.sh && ./count-server
```

This configuration guide provides all the information needed to tune Count for your specific deployment requirements. For production deployments, see the [Deployment Guide](deployment.md) for additional considerations.