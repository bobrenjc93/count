# Getting Started with Count

This guide will help you get Count up and running quickly, from a simple single-node setup to a multi-node distributed cluster.

## Prerequisites

- **Docker & Docker Compose** (recommended) - for containerized deployment
- **Rust 1.75+** (optional) - for building from source
- **curl** - for testing API endpoints

## Quick Start Options

### Option 1: Docker Compose (Recommended)

The fastest way to get a full 3-node cluster running:

```bash
# Clone the repository
git clone <repository-url>
cd count

# Start 3-node cluster
docker-compose up -d

# Check cluster health
curl http://localhost:8081/health
curl http://localhost:8082/health  
curl http://localhost:8083/health
```

This starts three nodes:
- Node 1: `http://localhost:8081`
- Node 2: `http://localhost:8082`
- Node 3: `http://localhost:8083`

### Option 2: Single Docker Container

For development or testing with a single node:

```bash
# Build the image
docker build -t count .

# Run single node
docker run -d \
  --name count-single \
  -p 8080:8080 \
  -e NODE_ID=1 \
  -e BIND_ADDRESS=0.0.0.0:8080 \
  -v count-data:/data \
  count

# Test the node
curl http://localhost:8080/health
```

### Option 3: Build from Source

For development or custom deployments:

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build the project
cargo build --release

# Run single node
export NODE_ID=1
export BIND_ADDRESS=0.0.0.0:8080
export DATA_DIR=./data
./target/release/count-server
```

## Your First Time Series Data

Once you have Count running, let's insert and query some data:

### 1. Insert Data Points

```bash
# Insert CPU usage data
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{
    "series": "cpu.usage.total",
    "timestamp": 1640995200000,
    "value": 75.5
  }'

# Insert memory usage data
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{
    "series": "memory.usage.percent", 
    "timestamp": 1640995200000,
    "value": 60.2
  }'

# Insert more data points over time
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{
    "series": "cpu.usage.total",
    "timestamp": 1640995260000,
    "value": 82.1
  }'
```

### 2. Query Data

#### Range Queries
Get all data points in a time range:

```bash
curl "http://localhost:8081/query/range/cpu.usage.total?start=1640995000000&end=1640995300000"
```

Response:
```json
[
  {"timestamp": 1640995200000, "value": 75.5},
  {"timestamp": 1640995260000, "value": 82.1}
]
```

#### Aggregated Queries
Compute aggregations over time ranges:

```bash
# Average CPU usage
curl "http://localhost:8081/query/aggregated/cpu.usage.total?start=1640995000000&end=1640995300000&aggregation=mean"

# Maximum CPU usage
curl "http://localhost:8081/query/aggregated/cpu.usage.total?start=1640995000000&end=1640995300000&aggregation=max"

# Total count of data points
curl "http://localhost:8081/query/aggregated/cpu.usage.total?start=1640995000000&end=1640995300000&aggregation=count"
```

## Understanding the Cluster

### Cluster Health

Check the health of your cluster:

```bash
# Check individual nodes
for port in 8081 8082 8083; do
  echo "Node on port $port:"
  curl -s http://localhost:$port/health | jq
  echo
done
```

Expected output:
```json
{
  "status": "healthy",
  "node_id": 1,
  "cluster_size": 3
}
```

### Data Distribution

In a multi-node cluster, data is automatically distributed based on the series name:

```bash
# These may end up on different nodes automatically
curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "server1.cpu", "timestamp": 1640995200000, "value": 45.0}'

curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "server2.cpu", "timestamp": 1640995200000, "value": 67.0}'

curl -X POST http://localhost:8081/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "server3.cpu", "timestamp": 1640995200000, "value": 23.0}'
```

You can query any node - it will automatically route to the correct data:

```bash
# Query from any node
curl "http://localhost:8082/query/range/server1.cpu?start=1640995000000&end=1640995300000"
curl "http://localhost:8083/query/range/server2.cpu?start=1640995000000&end=1640995300000"
```

## Configuration Basics

### Environment Variables

Key configuration options via environment variables:

```bash
# Node identification
export NODE_ID=1                    # Unique node identifier
export BIND_ADDRESS=0.0.0.0:8080    # Server bind address

# Cluster configuration  
export CLUSTER_ENABLED=true         # Enable clustering
export CLUSTER_NODES=node1:8080,node2:8080,node3:8080  # Cluster members
export REPLICATION_FACTOR=2         # Number of replicas

# Storage configuration
export DATA_DIR=/data/count         # Data storage directory
export MEMORY_BUFFER_SIZE=10000     # In-memory buffer size
export FLUSH_INTERVAL_SECONDS=300   # Flush interval (5 minutes)

# Logging
export RUST_LOG=info               # Log level
```

### Docker Compose Configuration

The `docker-compose.yml` file shows a complete cluster setup:

```yaml
version: '3.8'

services:
  count-node-1:
    build: .
    environment:
      - NODE_ID=1
      - CLUSTER_ENABLED=true
      - CLUSTER_NODES=count-node-1:8080,count-node-2:8080,count-node-3:8080
      - REPLICATION_FACTOR=2
      - BIND_ADDRESS=0.0.0.0:8080
    ports:
      - "8081:8080"
    volumes:
      - count-data-1:/data
```

## Sample Data and Workloads

### System Metrics Simulation

Here's a script to simulate realistic system metrics:

```bash
#!/bin/bash

# Function to generate timestamp
current_time() {
  date +%s000  # Convert to milliseconds
}

# Base timestamp (1 hour ago)
base_time=$(($(current_time) - 3600000))

# Insert CPU data over time
for i in {0..59}; do
  timestamp=$((base_time + i * 60000))  # Every minute
  cpu_value=$(echo "scale=1; 20 + ($RANDOM % 60)" | bc)
  
  curl -s -X POST http://localhost:8081/insert \
    -H "Content-Type: application/json" \
    -d "{\"series\": \"cpu.usage.total\", \"timestamp\": $timestamp, \"value\": $cpu_value}" \
    > /dev/null
    
  if [ $((i % 10)) -eq 0 ]; then
    echo "Inserted $i data points..."
  fi
done

echo "Data insertion complete!"

# Query the data
echo -e "\nQuerying average CPU usage:"
start_time=$base_time
end_time=$((base_time + 3600000))
curl -s "http://localhost:8081/query/aggregated/cpu.usage.total?start=$start_time&end=$end_time&aggregation=mean" | jq
```

### IoT Sensor Data

Simulate IoT sensor readings:

```bash
#!/bin/bash

# Temperature sensors
sensors=("sensor1.temperature" "sensor2.temperature" "sensor3.temperature")
base_time=$(($(date +%s000) - 1800000))  # 30 minutes ago

for sensor in "${sensors[@]}"; do
  for i in {0..30}; do
    timestamp=$((base_time + i * 60000))  # Every minute
    # Temperature between 18-25°C with some randomness
    temp=$(echo "scale=1; 18 + ($RANDOM % 70) / 10" | bc)
    
    curl -s -X POST http://localhost:8081/insert \
      -H "Content-Type: application/json" \
      -d "{\"series\": \"$sensor\", \"timestamp\": $timestamp, \"value\": $temp}" \
      > /dev/null
  done
  echo "Inserted data for $sensor"
done

# Query all sensors
echo -e "\nSensor readings summary:"
for sensor in "${sensors[@]}"; do
  avg=$(curl -s "http://localhost:8081/query/aggregated/$sensor?start=$base_time&end=$((base_time + 1800000))&aggregation=mean")
  echo "$sensor average: $avg°C"
done
```

## Next Steps

Now that you have Count running and understand the basics:

### Learn More
- **[Clustering Guide](clustering.md)** - Deep dive into multi-node operations
- **[API Reference](api-reference.md)** - Complete API documentation
- **[Configuration](configuration.md)** - All configuration options
- **[Architecture](architecture.md)** - How Count works internally

### Production Deployment
- **[Deployment Guide](deployment.md)** - Production setup recommendations
- **[Operations Guide](operations.md)** - Monitoring and maintenance
- **[Troubleshooting](troubleshooting.md)** - Common issues and solutions

### Development
- **[Development Guide](development.md)** - Contributing to Count
- Look at the example CLI in `src/bin/cli.rs`
- Review the integration tests in `tests/`

## Common First-Time Issues

### Port Conflicts
If ports 8081-8083 are in use:
```bash
# Check what's using the ports
lsof -i :8081

# Modify docker-compose.yml to use different ports
# Or stop conflicting services
```

### Docker Permission Issues
On Linux, you might need to add your user to the docker group:
```bash
sudo usermod -aG docker $USER
# Log out and back in
```

### Data Persistence
Docker volumes persist data between restarts:
```bash
# View volumes
docker volume ls

# Inspect volume
docker volume inspect count_count-data-1

# Remove all data (careful!)
docker-compose down -v
```

### Logs and Debugging
View logs for troubleshooting:
```bash
# View logs from all nodes
docker-compose logs

# Follow logs from specific node
docker-compose logs -f count-node-1

# Increase log verbosity
# Edit docker-compose.yml: RUST_LOG=debug
```

You're now ready to start using Count! The database will automatically handle data distribution, replication, and failover as you add more data and potentially expand your cluster.