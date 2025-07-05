# Troubleshooting Guide

This guide covers common issues you might encounter with Count, their symptoms, causes, and solutions.

## Common Issues

### 1. Node Won't Start

#### Symptoms
- Process exits immediately
- "Address already in use" error
- Configuration validation errors

#### Diagnosis
```bash
# Check if port is already in use
lsof -i :8080
netstat -tlnp | grep :8080

# Check configuration
env | grep -E "(NODE_ID|CLUSTER|BIND)"

# Check logs
docker logs count-node-1
# or
journalctl -u count-server
```

#### Solutions

**Port conflict:**
```bash
# Find process using the port
sudo lsof -i :8080
# Kill the process or change Count's port
export BIND_ADDRESS=0.0.0.0:8081
```

**Configuration issues:**
```bash
# Validate required environment variables
if [ -z "$NODE_ID" ]; then
    echo "ERROR: NODE_ID is required"
fi

# Check bind address format
if ! [[ "$BIND_ADDRESS" =~ ^[0-9.]+:[0-9]+$ ]]; then
    echo "ERROR: BIND_ADDRESS must be IP:PORT format"
fi
```

**Permission issues:**
```bash
# Check data directory permissions
ls -la /data/count
chown -R count:count /data/count
chmod 755 /data/count
```

### 2. Nodes Can't Join Cluster

#### Symptoms
- Nodes start individually but don't form cluster
- "Failed to join cluster" messages in logs
- Health endpoint shows `cluster_size: 1` on all nodes

#### Diagnosis
```bash
# Test network connectivity between nodes
telnet count-node-2 8080
ping count-node-2

# Check DNS resolution
nslookup count-node-2
dig count-node-2

# Verify cluster configuration
docker exec count-node-1 env | grep CLUSTER
```

#### Solutions

**Network connectivity:**
```bash
# Check firewall rules
iptables -L | grep 8080
ufw status

# Test connectivity
curl http://count-node-2:8080/health

# Check Docker networks
docker network ls
docker network inspect count_count-cluster
```

**DNS/hostname issues:**
```bash
# Add to /etc/hosts if needed
echo "192.168.1.11 count-node-2" >> /etc/hosts

# Or use IP addresses in CLUSTER_NODES
export CLUSTER_NODES=192.168.1.10:8080,192.168.1.11:8080,192.168.1.12:8080
```

**Configuration mismatch:**
```bash
# Ensure all nodes have identical CLUSTER_NODES
# Check each node's configuration
for node in count-node-1 count-node-2 count-node-3; do
    echo "=== $node ==="
    docker exec $node env | grep CLUSTER_NODES
done
```

### 3. High Write Latency

#### Symptoms
- Insert operations taking >100ms
- Client timeouts
- Growing write queue

#### Diagnosis
```bash
# Monitor system resources
htop
iotop
iostat 1

# Check disk I/O
iostat -x 1

# Monitor Count metrics
curl -w "@curl-format.txt" -X POST http://localhost:8080/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "test", "timestamp": 1640995200000, "value": 42}'
```

**curl-format.txt:**
```
     time_namelookup:  %{time_namelookup}\n
        time_connect:  %{time_connect}\n
     time_appconnect:  %{time_appconnect}\n
    time_pretransfer:  %{time_pretransfer}\n
       time_redirect:  %{time_redirect}\n
  time_starttransfer:  %{time_starttransfer}\n
                     ----------\n
          time_total:  %{time_total}\n
```

#### Solutions

**Increase memory buffer:**
```bash
export MEMORY_BUFFER_SIZE=50000
export FLUSH_INTERVAL_SECONDS=600
```

**Optimize storage:**
```bash
# Use faster storage (NVMe SSD)
# Mount with optimal flags
mount -o noatime,data=writeback /dev/nvme0n1 /data

# Increase OS buffers
echo 'vm.dirty_ratio = 15' >> /etc/sysctl.conf
echo 'vm.dirty_background_ratio = 5' >> /etc/sysctl.conf
sysctl -p
```

**Reduce replication factor temporarily:**
```bash
# For testing only - reduces durability
export REPLICATION_FACTOR=1
```

### 4. High Query Latency

#### Symptoms
- Query operations taking >1 second
- Query timeouts
- High CPU usage during queries

#### Diagnosis
```bash
# Test query performance
time curl "http://localhost:8080/query/range/test.series?start=0&end=9999999999999"

# Check data distribution
for port in 8081 8082 8083; do
    echo "Node $port:"
    curl -s "http://localhost:$port/query/aggregated/test.series?start=0&end=9999999999999&aggregation=count"
done

# Monitor CPU and memory
top -p $(pgrep count-server)
```

#### Solutions

**Optimize query ranges:**
```bash
# Use smaller time ranges
curl "http://localhost:8080/query/range/test.series?start=1640995000000&end=1640995300000"

# Use aggregations instead of raw data when possible
curl "http://localhost:8080/query/aggregated/test.series?start=1640995000000&end=1640995300000&aggregation=mean"
```

**Add more memory:**
```bash
# Increase system memory
# Or reduce memory buffer to leave more for OS cache
export MEMORY_BUFFER_SIZE=5000
```

**Scale horizontally:**
```bash
# Add more nodes to distribute query load
# Update CLUSTER_NODES and restart cluster
```

### 5. Data Inconsistency

#### Symptoms
- Different results from different nodes
- Missing data points
- Duplicate data points

#### Diagnosis
```bash
# Check data consistency across nodes
SERIES="test.consistency"
for port in 8081 8082 8083; do
    echo "Node $port:"
    curl -s "http://localhost:$port/query/aggregated/$SERIES?start=0&end=9999999999999&aggregation=count"
done

# Check replication status in logs
docker logs count-node-1 | grep -i replication
```

#### Solutions

**Manual data repair:**
```bash
#!/bin/bash
# repair-data.sh
SERIES="problematic.series"
START_TIME=1640995000000
END_TIME=1640998600000

# Get data from all nodes
for port in 8081 8082 8083; do
    curl -s "http://localhost:$port/query/range/$SERIES?start=$START_TIME&end=$END_TIME" > node_$port.json
done

# Compare and identify inconsistencies
# Manual intervention may be required
```

**Restart cluster with clean state:**
```bash
# CAUTION: This will lose data
docker-compose down
docker volume rm count_count-data-1 count_count-data-2 count_count-data-3
docker-compose up -d
```

### 6. Memory Issues

#### Symptoms
- Out of memory errors
- Process killed by OOM killer
- Excessive memory usage

#### Diagnosis
```bash
# Check memory usage
free -h
ps aux | grep count-server

# Check Count memory usage
curl http://localhost:8080/health  # Future: will include memory stats

# Check for memory leaks
valgrind --tool=memcheck --leak-check=full ./count-server
```

#### Solutions

**Reduce memory buffer:**
```bash
export MEMORY_BUFFER_SIZE=1000
export FLUSH_INTERVAL_SECONDS=60
```

**Add swap (temporary solution):**
```bash
# Create swap file (not recommended for production)
fallocate -l 2G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
```

**Scale horizontally:**
```bash
# Add more nodes to distribute memory load
# Each node will handle less data
```

### 7. Disk Space Issues

#### Symptoms
- "No space left on device" errors
- Write operations failing
- Process unable to start

#### Diagnosis
```bash
# Check disk usage
df -h
du -sh /data/count/*

# Check for large log files
du -sh /var/log/count/
```

#### Solutions

**Clean up old data:**
```bash
# Implement data retention policy
find /data/count -name "*.block" -mtime +30 -delete

# Rotate logs
logrotate -f /etc/logrotate.d/count
```

**Expand storage:**
```bash
# Add more storage
# Or move to larger volume
rsync -av /data/count/ /new/larger/volume/count/
```

**Implement compression:**
```bash
# Count already uses compression, but you can:
# - Reduce flush interval to compress sooner
export FLUSH_INTERVAL_SECONDS=60

# - Use filesystem compression (ZFS, Btrfs)
```

## Network Issues

### 1. Connection Refused

#### Symptoms
- `curl: (7) Failed to connect`
- `Connection refused` errors

#### Solutions
```bash
# Check if service is running
ps aux | grep count-server
systemctl status count-server

# Check if port is open
ss -tlnp | grep :8080

# Check firewall
iptables -L
ufw status

# Test locally first
curl http://127.0.0.1:8080/health
```

### 2. Timeouts

#### Symptoms
- Request timeouts
- Partial responses
- Intermittent failures

#### Solutions
```bash
# Increase client timeout
curl --max-time 60 http://localhost:8080/query/...

# Check network latency
ping count-node-2
traceroute count-node-2

# Monitor network usage
iftop
nethogs
```

### 3. DNS Issues

#### Symptoms
- "Could not resolve host" errors
- Inconsistent connectivity

#### Solutions
```bash
# Test DNS resolution
nslookup count-node-2
dig count-node-2

# Use IP addresses instead
export CLUSTER_NODES=192.168.1.10:8080,192.168.1.11:8080

# Add to /etc/hosts
echo "192.168.1.11 count-node-2" >> /etc/hosts
```

## Container Issues

### 1. Container Won't Start

#### Symptoms
- Container exits immediately
- "docker: Error response from daemon"

#### Solutions
```bash
# Check container logs
docker logs count-node-1

# Check image
docker images | grep count

# Check Docker daemon
systemctl status docker

# Try running interactively
docker run -it count:latest /bin/bash
```

### 2. Volume Issues

#### Symptoms
- Data not persisting
- Permission denied errors

#### Solutions
```bash
# Check volume mounts
docker inspect count-node-1 | jq '.[0].Mounts'

# Check permissions
docker exec count-node-1 ls -la /data

# Fix permissions
docker exec count-node-1 chown -R count:count /data
```

### 3. Network Issues

#### Symptoms
- Containers can't communicate
- Services unreachable

#### Solutions
```bash
# Check Docker networks
docker network ls
docker network inspect count_count-cluster

# Test connectivity between containers
docker exec count-node-1 ping count-node-2
docker exec count-node-1 curl http://count-node-2:8080/health
```

## Performance Debugging

### 1. CPU Profiling

```bash
# Install profiling tools
cargo install flamegraph

# Profile the application
# For development build:
cargo flamegraph --bin count-server

# For production:
perf record -g ./count-server
perf report
```

### 2. Memory Profiling

```bash
# Use valgrind (Linux only)
valgrind --tool=massif ./count-server

# Use system tools
pmap $(pgrep count-server)
cat /proc/$(pgrep count-server)/status
```

### 3. I/O Profiling

```bash
# Monitor disk I/O
iostat -x 1
iotop -p $(pgrep count-server)

# Monitor network I/O
iftop
ss -i
```

## Diagnostic Scripts

### Health Check Script

```bash
#!/bin/bash
# comprehensive-health-check.sh

echo "Count Cluster Health Check"
echo "========================="

# Check system resources
echo "System Resources:"
echo "CPU: $(uptime | awk -F'load average:' '{print $2}')"
echo "Memory: $(free -h | grep '^Mem:' | awk '{print $3 "/" $2}')"
echo "Disk: $(df -h /data | tail -1 | awk '{print $3 "/" $2 " (" $5 " used)"}')"
echo

# Check Count processes
echo "Count Processes:"
ps aux | grep -E "(count-server|count-cli)" | grep -v grep
echo

# Check network connectivity
echo "Network Connectivity:"
NODES=("localhost:8081" "localhost:8082" "localhost:8083")
for node in "${NODES[@]}"; do
    if curl -s --max-time 5 "http://$node/health" > /dev/null; then
        echo "✅ $node - OK"
    else
        echo "❌ $node - FAILED"
    fi
done
echo

# Check cluster health
echo "Cluster Health:"
for port in 8081 8082 8083; do
    response=$(curl -s --max-time 5 "http://localhost:$port/health" 2>/dev/null)
    if [ $? -eq 0 ]; then
        status=$(echo "$response" | jq -r '.status // "unknown"')
        cluster_size=$(echo "$response" | jq -r '.cluster_size // 0')
        node_id=$(echo "$response" | jq -r '.node_id // "unknown"')
        echo "Node $node_id (port $port): $status (cluster size: $cluster_size)"
    else
        echo "Node on port $port: UNREACHABLE"
    fi
done
```

### Performance Test Script

```bash
#!/bin/bash
# performance-test.sh

NODES=("localhost:8081" "localhost:8082" "localhost:8083")
TEST_SERIES="perf_test_$(date +%s)"
NUM_POINTS=1000

echo "Performance Test: $TEST_SERIES"
echo "Testing $NUM_POINTS data points across ${#NODES[@]} nodes"

# Write test
echo "Write Performance Test:"
start_time=$(date +%s%N)
for i in $(seq 1 $NUM_POINTS); do
    node=${NODES[$((i % ${#NODES[@]}))]}
    timestamp=$(($(date +%s000) + i * 1000))
    
    curl -s -X POST "http://$node/insert" \
        -H "Content-Type: application/json" \
        -d "{\"series\": \"$TEST_SERIES\", \"timestamp\": $timestamp, \"value\": $i}" \
        > /dev/null &
        
    # Limit concurrent requests
    if [ $((i % 50)) -eq 0 ]; then
        wait
    fi
done
wait

end_time=$(date +%s%N)
write_duration=$(((end_time - start_time) / 1000000))  # Convert to ms
write_throughput=$((NUM_POINTS * 1000 / write_duration))

echo "Write Duration: ${write_duration}ms"
echo "Write Throughput: ${write_throughput} points/sec"

# Read test
echo
echo "Read Performance Test:"
sleep 5  # Wait for data to be available

start_time=$(date +%s%N)
for node in "${NODES[@]}"; do
    curl -s "http://$node/query/range/$TEST_SERIES?start=0&end=9999999999999" > /dev/null &
done
wait

end_time=$(date +%s%N)
read_duration=$(((end_time - start_time) / 1000000))

echo "Read Duration: ${read_duration}ms"

# Cleanup
echo
echo "Cleaning up test data..."
# Note: Count doesn't have delete API yet, data will be cleaned up by retention policy
```

## Getting Help

### Log Collection

```bash
#!/bin/bash
# collect-logs.sh

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_DIR="/tmp/count_logs_$TIMESTAMP"

mkdir -p "$LOG_DIR"

echo "Collecting Count diagnostic information..."

# System information
uname -a > "$LOG_DIR/system_info.txt"
free -h >> "$LOG_DIR/system_info.txt"
df -h >> "$LOG_DIR/system_info.txt"
ps aux | grep count >> "$LOG_DIR/system_info.txt"

# Count logs
if [ -d "/var/log/count" ]; then
    cp -r /var/log/count "$LOG_DIR/"
fi

# Docker logs (if using Docker)
for container in count-node-1 count-node-2 count-node-3; do
    if docker ps | grep -q "$container"; then
        docker logs "$container" > "$LOG_DIR/${container}.log" 2>&1
    fi
done

# Configuration
env | grep -E "(COUNT|NODE|CLUSTER|BIND)" > "$LOG_DIR/environment.txt"

# Network information
ss -tlnp | grep :8080 > "$LOG_DIR/network.txt"
iptables -L >> "$LOG_DIR/network.txt" 2>/dev/null || echo "No iptables access" >> "$LOG_DIR/network.txt"

# Create archive
tar czf "count_diagnostics_$TIMESTAMP.tar.gz" -C /tmp "count_logs_$TIMESTAMP"
rm -rf "$LOG_DIR"

echo "Diagnostic information saved to: count_diagnostics_$TIMESTAMP.tar.gz"
```

### Useful Resources

- **GitHub Issues**: Report bugs and feature requests
- **Documentation**: Check the complete documentation set
- **Community**: Join discussions and get help from other users
- **Performance**: Review benchmarks and optimization guides

### Emergency Contacts

For production issues:
1. Check this troubleshooting guide
2. Collect diagnostic information
3. Review recent changes
4. Contact your system administrator
5. If data loss is imminent, stop the cluster and backup volumes

Remember: It's better to have a temporarily unavailable cluster than to lose data permanently.