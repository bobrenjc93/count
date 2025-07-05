# Operations Guide

This guide covers day-to-day operational tasks for managing Count clusters in production, including monitoring, maintenance, scaling, and troubleshooting.

## Health Monitoring

### Cluster Health Dashboard

**Basic health check script:**
```bash
#!/bin/bash
# health-dashboard.sh

NODES=("localhost:8081" "localhost:8082" "localhost:8083")
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')

echo "Count Cluster Health Report - $TIMESTAMP"
echo "=================================================="
echo

total_nodes=0
healthy_nodes=0
cluster_size=0

for node in "${NODES[@]}"; do
    total_nodes=$((total_nodes + 1))
    
    response=$(curl -s -w "%{http_code}" -o /tmp/health_$total_nodes.json http://$node/health 2>/dev/null)
    
    if [ "$response" = "200" ]; then
        status=$(jq -r '.status' /tmp/health_$total_nodes.json 2>/dev/null)
        node_id=$(jq -r '.node_id' /tmp/health_$total_nodes.json 2>/dev/null)
        cluster_size=$(jq -r '.cluster_size' /tmp/health_$total_nodes.json 2>/dev/null)
        
        if [ "$status" = "healthy" ]; then
            healthy_nodes=$((healthy_nodes + 1))
            echo "✅ Node $node_id ($node): HEALTHY"
        else
            echo "⚠️  Node $node_id ($node): $status"
        fi
    else
        echo "❌ Node $node: UNREACHABLE (HTTP $response)"
    fi
done

echo
echo "Summary:"
echo "--------"
echo "Total Nodes: $total_nodes"
echo "Healthy Nodes: $healthy_nodes"
echo "Cluster Size: $cluster_size"

if [ "$healthy_nodes" -eq "$total_nodes" ] && [ "$cluster_size" -ge 2 ]; then
    echo "Status: ✅ CLUSTER HEALTHY"
    exit 0
elif [ "$healthy_nodes" -ge "$((total_nodes / 2 + 1))" ]; then
    echo "Status: ⚠️  CLUSTER DEGRADED"
    exit 1
else
    echo "Status: ❌ CLUSTER CRITICAL"
    exit 2
fi
```

### Automated Monitoring

**Systemd service for continuous monitoring:**
```ini
# /etc/systemd/system/count-monitor.service
[Unit]
Description=Count Cluster Monitor
After=network.target

[Service]
Type=simple
User=count
ExecStart=/opt/count/scripts/health-monitor.sh
Restart=always
RestartSec=30

[Install]
WantedBy=multi-user.target
```

**Health monitor script:**
```bash
#!/bin/bash
# /opt/count/scripts/health-monitor.sh

NODES=("count-1:8080" "count-2:8080" "count-3:8080")
ALERT_THRESHOLD=2  # Alert after 2 consecutive failures
LOG_FILE="/var/log/count/monitor.log"
ALERT_EMAIL="ops@yourdomain.com"

declare -A failure_counts

log_message() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

send_alert() {
    local subject="$1"
    local message="$2"
    echo "$message" | mail -s "$subject" "$ALERT_EMAIL"
    log_message "ALERT SENT: $subject"
}

check_node() {
    local node="$1"
    local response
    response=$(curl -s -w "%{http_code}" -o /dev/null --max-time 10 "http://$node/health" 2>/dev/null)
    
    if [ "$response" = "200" ]; then
        if [ "${failure_counts[$node]}" -gt 0 ]; then
            log_message "RECOVERY: Node $node is healthy again"
            send_alert "Count Node Recovery" "Node $node has recovered and is now healthy."
        fi
        failure_counts[$node]=0
        return 0
    else
        failure_counts[$node]=$((failure_counts[$node] + 1))
        log_message "FAILURE: Node $node check failed (count: ${failure_counts[$node]})"
        
        if [ "${failure_counts[$node]}" -eq "$ALERT_THRESHOLD" ]; then
            send_alert "Count Node Failure" "Node $node has failed health checks $ALERT_THRESHOLD times consecutively."
        fi
        return 1
    fi
}

# Main monitoring loop
log_message "Starting Count cluster monitoring"

while true; do
    healthy_count=0
    total_count=${#NODES[@]}
    
    for node in "${NODES[@]}"; do
        if check_node "$node"; then
            healthy_count=$((healthy_count + 1))
        fi
    done
    
    # Check cluster health
    if [ "$healthy_count" -lt "$((total_count / 2 + 1))" ]; then
        send_alert "Count Cluster Critical" "Only $healthy_count out of $total_count nodes are healthy. Cluster may be unavailable."
    fi
    
    sleep 60  # Check every minute
done
```

### Grafana Dashboard

**Prometheus metrics (when available):**
```yaml
# docker-compose.monitoring.yml
version: '3.8'

services:
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    volumes:
      - grafana-data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin

volumes:
  prometheus-data:
  grafana-data:
```

**Sample Grafana dashboard config:**
```json
{
  "dashboard": {
    "title": "Count Cluster Overview",
    "panels": [
      {
        "title": "Cluster Health",
        "type": "stat",
        "targets": [
          {
            "expr": "up{job=\"count\"}",
            "legendFormat": "Node {{instance}}"
          }
        ]
      },
      {
        "title": "Write Throughput",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(count_writes_total[5m])",
            "legendFormat": "Writes/sec"
          }
        ]
      },
      {
        "title": "Query Latency", 
        "type": "graph",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, rate(count_query_duration_seconds_bucket[5m]))",
            "legendFormat": "95th percentile"
          }
        ]
      }
    ]
  }
}
```

## Log Management

### Log Collection and Analysis

**Centralized logging setup:**
```bash
#!/bin/bash
# setup-logging.sh

# Create log directories
mkdir -p /var/log/count
chown count:count /var/log/count

# Configure logrotate
cat > /etc/logrotate.d/count << 'EOF'
/var/log/count/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 644 count count
    postrotate
        systemctl reload count || true
    endscript
}
EOF

# Test logrotate
logrotate -d /etc/logrotate.d/count
```

**Log analysis scripts:**
```bash
#!/bin/bash
# analyze-logs.sh

LOG_DIR="/var/log/count"
REPORT_FILE="/tmp/count-log-report.txt"

echo "Count Log Analysis Report - $(date)" > "$REPORT_FILE"
echo "=================================" >> "$REPORT_FILE"
echo >> "$REPORT_FILE"

# Error analysis
echo "Recent Errors:" >> "$REPORT_FILE"
grep -i error "$LOG_DIR"/*.log | tail -20 >> "$REPORT_FILE"
echo >> "$REPORT_FILE"

# Performance metrics
echo "Performance Metrics:" >> "$REPORT_FILE"
grep -E "(insert|query).*latency" "$LOG_DIR"/*.log | \
    awk '{print $NF}' | sort -n | tail -10 >> "$REPORT_FILE"
echo >> "$REPORT_FILE"

# Cluster events
echo "Cluster Events:" >> "$REPORT_FILE"
grep -E "(node.*join|node.*leave|node.*fail)" "$LOG_DIR"/*.log | tail -10 >> "$REPORT_FILE"
echo >> "$REPORT_FILE"

# Show report
cat "$REPORT_FILE"
```

### Log Forwarding

**Fluent Bit configuration:**
```yaml
# fluent-bit.conf
[SERVICE]
    Flush         5
    Daemon        Off
    Log_Level     info

[INPUT]
    Name              tail
    Path              /var/log/count/*.log
    Tag               count
    Refresh_Interval  5
    Skip_Long_Lines   On
    
[FILTER]
    Name parser
    Match count
    Key_Name log  
    Parser json

[OUTPUT]
    Name  es
    Match count
    Host  elasticsearch.logging.svc.cluster.local
    Port  9200
    Index count-logs
    Type  _doc
```

## Performance Monitoring

### Resource Usage Tracking

**System resource monitor:**
```bash
#!/bin/bash
# resource-monitor.sh

OUTPUT_FILE="/var/log/count/resources.log"

while true; do
    TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
    
    # CPU usage
    CPU_USAGE=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)
    
    # Memory usage
    MEM_INFO=$(free -m | grep '^Mem:')
    MEM_TOTAL=$(echo $MEM_INFO | awk '{print $2}')
    MEM_USED=$(echo $MEM_INFO | awk '{print $3}')
    MEM_PERCENT=$(( MEM_USED * 100 / MEM_TOTAL ))
    
    # Disk usage
    DISK_USAGE=$(df -h /data/count | tail -1 | awk '{print $5}' | cut -d'%' -f1)
    
    # Network I/O
    NETWORK_INFO=$(cat /proc/net/dev | grep eth0)
    RX_BYTES=$(echo $NETWORK_INFO | awk '{print $2}')
    TX_BYTES=$(echo $NETWORK_INFO | awk '{print $10}')
    
    # Count-specific metrics
    COUNT_PID=$(pgrep count-server)
    if [ -n "$COUNT_PID" ]; then
        COUNT_MEM=$(ps -p $COUNT_PID -o rss= | awk '{print $1/1024}')  # MB
        COUNT_CPU=$(ps -p $COUNT_PID -o pcpu= | awk '{print $1}')
    else
        COUNT_MEM=0
        COUNT_CPU=0
    fi
    
    # Log metrics
    echo "$TIMESTAMP,CPU:${CPU_USAGE}%,MEM:${MEM_PERCENT}%,DISK:${DISK_USAGE}%,COUNT_MEM:${COUNT_MEM}MB,COUNT_CPU:${COUNT_CPU}%" >> "$OUTPUT_FILE"
    
    sleep 60
done
```

### Performance Benchmarking

**Load testing script:**
```bash
#!/bin/bash
# load-test.sh

COUNT_URL="http://localhost:8080"
NUM_SERIES=100
POINTS_PER_SERIES=1000
CONCURRENT_WRITERS=10

echo "Starting Count load test..."
echo "Series: $NUM_SERIES"
echo "Points per series: $POINTS_PER_SERIES"
echo "Concurrent writers: $CONCURRENT_WRITERS"

# Generate test data
generate_series_data() {
    local series_id=$1
    local start_time=$(date +%s000)
    
    for i in $(seq 1 $POINTS_PER_SERIES); do
        local timestamp=$((start_time + i * 1000))
        local value=$((RANDOM % 100))
        
        curl -s -X POST "$COUNT_URL/insert" \
            -H "Content-Type: application/json" \
            -d "{\"series\": \"load_test_$series_id\", \"timestamp\": $timestamp, \"value\": $value}" \
            > /dev/null
            
        if [ $((i % 100)) -eq 0 ]; then
            echo "Series $series_id: $i/$POINTS_PER_SERIES points inserted"
        fi
    done
}

# Start timing
START_TIME=$(date +%s)

# Launch concurrent writers
for series_id in $(seq 1 $NUM_SERIES); do
    if [ $((series_id % CONCURRENT_WRITERS)) -eq 0 ]; then
        wait  # Wait for previous batch to complete
    fi
    generate_series_data $series_id &
done

wait  # Wait for all writers to complete

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
TOTAL_POINTS=$((NUM_SERIES * POINTS_PER_SERIES))
THROUGHPUT=$((TOTAL_POINTS / DURATION))

echo
echo "Load test completed!"
echo "Duration: ${DURATION}s"
echo "Total points: $TOTAL_POINTS" 
echo "Throughput: ${THROUGHPUT} points/sec"

# Verify data
echo
echo "Verifying data integrity..."
QUERY_START=$(date +%s000)
QUERY_END=$((QUERY_START + 3600000))

for series_id in $(seq 1 5); do  # Check first 5 series
    COUNT=$(curl -s "$COUNT_URL/query/aggregated/load_test_$series_id?start=$QUERY_START&end=$QUERY_END&aggregation=count")
    echo "Series load_test_$series_id: $COUNT points"
done
```

## Scaling Operations

### Adding Nodes

**Node addition procedure:**
```bash
#!/bin/bash
# add-node.sh

NEW_NODE_ID=$1
NEW_NODE_ADDRESS=$2

if [ -z "$NEW_NODE_ID" ] || [ -z "$NEW_NODE_ADDRESS" ]; then
    echo "Usage: $0 <node_id> <node_address>"
    echo "Example: $0 4 192.168.1.14:8080"
    exit 1
fi

echo "Adding new node: ID=$NEW_NODE_ID, Address=$NEW_NODE_ADDRESS"

# Step 1: Update cluster configuration
CURRENT_NODES=$(curl -s http://localhost:8081/health | jq -r '.cluster_nodes // empty')
if [ -z "$CURRENT_NODES" ]; then
    echo "Error: Could not retrieve current cluster nodes"
    exit 1
fi

NEW_CLUSTER_NODES="$CURRENT_NODES,$NEW_NODE_ADDRESS"
echo "New cluster configuration: $NEW_CLUSTER_NODES"

# Step 2: Prepare new node configuration
cat > /tmp/new-node-config.env << EOF
NODE_ID=$NEW_NODE_ID
CLUSTER_ENABLED=true
CLUSTER_NODES=$NEW_CLUSTER_NODES
REPLICATION_FACTOR=2
BIND_ADDRESS=0.0.0.0:8080
DATA_DIR=/data
RUST_LOG=info
EOF

echo "Generated configuration for new node:"
cat /tmp/new-node-config.env

# Step 3: Instructions for operator
echo
echo "Next steps:"
echo "1. Deploy new node with the configuration above"
echo "2. Wait for node to join cluster"
echo "3. Update existing nodes with new CLUSTER_NODES configuration"
echo "4. Restart existing nodes one by one"
echo "5. Verify cluster health"
```

**Rolling update script:**
```bash
#!/bin/bash
# rolling-update.sh

NODES=("count-node-1" "count-node-2" "count-node-3")
NEW_CLUSTER_NODES="count-node-1:8080,count-node-2:8080,count-node-3:8080,count-node-4:8080"

echo "Starting rolling update to add node-4..."

for node in "${NODES[@]}"; do
    echo "Updating $node..."
    
    # Update environment configuration
    docker-compose stop "$node"
    
    # Update docker-compose.yml with new CLUSTER_NODES
    sed -i "s/CLUSTER_NODES=.*/CLUSTER_NODES=$NEW_CLUSTER_NODES/" docker-compose.yml
    
    # Start node with new configuration
    docker-compose up -d "$node"
    
    # Wait for node to be healthy
    echo "Waiting for $node to become healthy..."
    for i in {1..30}; do
        if curl -s "http://localhost:808${node: -1}/health" > /dev/null 2>&1; then
            echo "$node is healthy"
            break
        fi
        sleep 10
    done
    
    echo "Waiting 30 seconds before updating next node..."
    sleep 30
done

echo "Rolling update completed"
```

### Removing Nodes

**Node removal procedure:**
```bash
#!/bin/bash
# remove-node.sh

NODE_TO_REMOVE=$1

if [ -z "$NODE_TO_REMOVE" ]; then
    echo "Usage: $0 <node_name>"
    echo "Example: $0 count-node-3"
    exit 1
fi

echo "Removing node: $NODE_TO_REMOVE"

# Step 1: Stop accepting new writes (implement at load balancer level)
echo "1. Configure load balancer to stop sending traffic to $NODE_TO_REMOVE"
read -p "Press Enter when load balancer is updated..."

# Step 2: Graceful shutdown
echo "2. Gracefully shutting down $NODE_TO_REMOVE"
docker-compose stop "$NODE_TO_REMOVE"

# Step 3: Wait for data replication
echo "3. Waiting for data replication to other nodes..."
sleep 60

# Step 4: Update cluster configuration
echo "4. Updating remaining nodes with new cluster configuration"
# This would involve updating CLUSTER_NODES and restarting remaining nodes

# Step 5: Cleanup
echo "5. Cleaning up node resources"
docker-compose rm -f "$NODE_TO_REMOVE"

echo "Node removal completed"
```

## Backup and Recovery

### Automated Backup System

**Backup automation script:**
```bash
#!/bin/bash
# automated-backup.sh

BACKUP_DIR="/backups/count"
RETENTION_DAYS=30
S3_BUCKET="your-backup-bucket"
DATE=$(date +%Y-%m-%d-%H-%M-%S)
BACKUP_PATH="$BACKUP_DIR/$DATE"

log_message() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a /var/log/count/backup.log
}

log_message "Starting backup process"

# Create backup directory
mkdir -p "$BACKUP_PATH"

# Pre-backup health check
CLUSTER_HEALTH=$(curl -s http://localhost:8081/health | jq -r '.status')
if [ "$CLUSTER_HEALTH" != "healthy" ]; then
    log_message "WARNING: Cluster not healthy, backup may be inconsistent"
fi

# Backup each node's data
NODES=("count-node-1" "count-node-2" "count-node-3")
for node in "${NODES[@]}"; do
    log_message "Backing up $node"
    
    # Use docker cp to avoid stopping services
    CONTAINER_ID=$(docker-compose ps -q "$node")
    if [ -n "$CONTAINER_ID" ]; then
        docker exec "$CONTAINER_ID" tar czf "/tmp/backup.tar.gz" -C "/data" .
        docker cp "$CONTAINER_ID:/tmp/backup.tar.gz" "$BACKUP_PATH/${node}-data.tar.gz"
        docker exec "$CONTAINER_ID" rm "/tmp/backup.tar.gz"
        log_message "Completed backup for $node"
    else
        log_message "ERROR: Could not find container for $node"
    fi
done

# Create backup metadata
cat > "$BACKUP_PATH/metadata.json" << EOF
{
    "timestamp": "$DATE",
    "cluster_health": "$CLUSTER_HEALTH",
    "backup_type": "automated",
    "nodes": $(printf '%s\n' "${NODES[@]}" | jq -R . | jq -s .)
}
EOF

# Upload to S3 (optional)
if [ -n "$S3_BUCKET" ]; then
    log_message "Uploading backup to S3"
    aws s3 sync "$BACKUP_PATH" "s3://$S3_BUCKET/count/$DATE/" --quiet
    if [ $? -eq 0 ]; then
        log_message "S3 upload completed"
    else
        log_message "ERROR: S3 upload failed"
    fi
fi

# Cleanup old backups
log_message "Cleaning up old backups"
find "$BACKUP_DIR" -type d -name "20*" -mtime +$RETENTION_DAYS -exec rm -rf {} \; 2>/dev/null

# Calculate backup size
BACKUP_SIZE=$(du -sh "$BACKUP_PATH" | cut -f1)
log_message "Backup completed: $BACKUP_PATH ($BACKUP_SIZE)"
```

**Cron configuration:**
```bash
# /etc/cron.d/count-backup
# Daily backup at 2 AM
0 2 * * * count /opt/count/scripts/automated-backup.sh

# Weekly full backup at 1 AM Sunday
0 1 * * 0 count /opt/count/scripts/full-backup.sh
```

### Recovery Procedures

**Point-in-time recovery:**
```bash
#!/bin/bash
# point-in-time-recovery.sh

BACKUP_DATE=$1
BACKUP_DIR="/backups/count"

if [ -z "$BACKUP_DATE" ]; then
    echo "Available backups:"
    ls -la "$BACKUP_DIR" | grep "^d" | grep "20"
    echo
    echo "Usage: $0 <backup_date>"
    echo "Example: $0 2024-01-15-02-00-00"
    exit 1
fi

BACKUP_PATH="$BACKUP_DIR/$BACKUP_DATE"

if [ ! -d "$BACKUP_PATH" ]; then
    echo "Error: Backup not found at $BACKUP_PATH"
    exit 1
fi

echo "Starting point-in-time recovery from $BACKUP_DATE"
echo "WARNING: This will destroy current data!"
read -p "Are you sure? (yes/no): " confirm

if [ "$confirm" != "yes" ]; then
    echo "Recovery cancelled"
    exit 0
fi

# Stop cluster
echo "Stopping Count cluster..."
docker-compose down

# Remove existing data
echo "Removing existing data..."
docker volume rm count_count-data-1 count_count-data-2 count_count-data-3 2>/dev/null || true

# Restore from backup
NODES=("count-node-1" "count-node-2" "count-node-3")
for i in "${!NODES[@]}"; do
    node_num=$((i + 1))
    node="${NODES[$i]}"
    
    echo "Restoring $node..."
    
    docker volume create "count_count-data-$node_num"
    docker run --rm \
        -v "count_count-data-$node_num:/target" \
        -v "$BACKUP_PATH:/backup:ro" \
        alpine tar xzf "/backup/${node}-data.tar.gz" -C /target
done

# Start cluster
echo "Starting Count cluster..."
docker-compose up -d

# Wait for cluster to be ready
echo "Waiting for cluster to initialize..."
sleep 30

# Verify recovery
echo "Verifying recovery..."
for port in 8081 8082 8083; do
    response=$(curl -s "http://localhost:$port/health")
    if echo "$response" | jq -e '.status == "healthy"' > /dev/null 2>&1; then
        echo "✅ Node on port $port is healthy"
    else
        echo "❌ Node on port $port is not healthy"
    fi
done

echo "Recovery completed!"
```

## Troubleshooting

### Common Issues and Solutions

**1. Node won't join cluster:**
```bash
# Diagnostic script
#!/bin/bash
# diagnose-cluster-join.sh

NODE_IP=$1
NODE_PORT=${2:-8080}

echo "Diagnosing cluster join issues for $NODE_IP:$NODE_PORT"

# Check network connectivity
echo "1. Testing network connectivity..."
if telnet "$NODE_IP" "$NODE_PORT" </dev/null 2>/dev/null; then
    echo "✅ Network connectivity OK"
else
    echo "❌ Cannot connect to $NODE_IP:$NODE_PORT"
    echo "   Check firewall rules and network configuration"
fi

# Check DNS resolution
echo "2. Testing DNS resolution..."
if nslookup "$NODE_IP" >/dev/null 2>&1; then
    echo "✅ DNS resolution OK"
else
    echo "⚠️  DNS resolution failed, using IP address"
fi

# Check configuration
echo "3. Checking configuration..."
curl -s "http://$NODE_IP:$NODE_PORT/health" | jq . || echo "❌ Node not responding"

# Check logs
echo "4. Recent logs from node:"
docker logs --tail 20 count-node-$(echo $NODE_IP | cut -d'.' -f4) 2>/dev/null || echo "Cannot access logs"
```

**2. High latency issues:**
```bash
#!/bin/bash
# diagnose-latency.sh

echo "Diagnosing Count latency issues..."

# Test write latency
echo "Testing write latency..."
for i in {1..10}; do
    timestamp=$(date +%s000)
    start_time=$(date +%s%N)
    
    curl -s -X POST http://localhost:8081/insert \
        -H "Content-Type: application/json" \
        -d "{\"series\": \"latency_test\", \"timestamp\": $timestamp, \"value\": $i}"
    
    end_time=$(date +%s%N)
    latency=$(( (end_time - start_time) / 1000000 ))  # Convert to ms
    echo "Write $i: ${latency}ms"
done

# Test read latency
echo
echo "Testing read latency..."
for i in {1..10}; do
    start_time=$(date +%s%N)
    
    curl -s "http://localhost:8081/query/range/latency_test?start=0&end=9999999999999" > /dev/null
    
    end_time=$(date +%s%N)
    latency=$(( (end_time - start_time) / 1000000 ))
    echo "Read $i: ${latency}ms"
done
```

**3. Data inconsistency detection:**
```bash
#!/bin/bash
# detect-inconsistency.sh

SERIES="consistency_test"
NODES=("localhost:8081" "localhost:8082" "localhost:8083")

echo "Detecting data inconsistencies..."

# Insert test data to one node
echo "Inserting test data..."
curl -s -X POST "http://${NODES[0]}/insert" \
    -H "Content-Type: application/json" \
    -d "{\"series\": \"$SERIES\", \"timestamp\": $(date +%s000), \"value\": 42}"

# Wait for replication
sleep 5

# Query all nodes
echo "Querying all nodes..."
for i in "${!NODES[@]}"; do
    node="${NODES[$i]}"
    result=$(curl -s "http://$node/query/range/$SERIES?start=0&end=9999999999999")
    count=$(echo "$result" | jq length)
    echo "Node $((i+1)) ($node): $count points"
    
    if [ "$i" -eq 0 ]; then
        expected_count=$count
    elif [ "$count" -ne "$expected_count" ]; then
        echo "⚠️  INCONSISTENCY DETECTED: Expected $expected_count, got $count"
    fi
done
```

### Emergency Procedures

**Cluster split-brain recovery:**
```bash
#!/bin/bash
# recover-split-brain.sh

echo "EMERGENCY: Split-brain recovery procedure"
echo "This should only be used when cluster is completely unavailable"
read -p "Continue? (yes/no): " confirm

if [ "$confirm" != "yes" ]; then
    exit 0
fi

# Stop all nodes
echo "1. Stopping all nodes..."
docker-compose down

# Choose authoritative node (typically the one with most recent data)
echo "2. Starting authoritative node..."
docker-compose up -d count-node-1

# Wait for single node to be ready
sleep 30

# Verify single node is working
curl http://localhost:8081/health

# Start other nodes one by one
echo "3. Starting remaining nodes..."
docker-compose up -d count-node-2
sleep 30
docker-compose up -d count-node-3
sleep 30

# Verify cluster recovery
echo "4. Verifying cluster recovery..."
for port in 8081 8082 8083; do
    curl -s "http://localhost:$port/health" | jq .
done
```

This operations guide provides the tools and procedures needed for day-to-day management of Count clusters in production. Regular monitoring and proactive maintenance will help ensure optimal performance and availability.