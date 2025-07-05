# Deployment Guide

This guide covers production deployment strategies for Count, including infrastructure requirements, deployment patterns, and operational considerations.

## Infrastructure Requirements

### Hardware Specifications

#### Minimum Requirements (Development/Testing)
- **CPU:** 2 cores
- **RAM:** 2GB
- **Storage:** 10GB SSD
- **Network:** 100 Mbps
- **Expected Load:** ~1K writes/sec, ~500 queries/sec

#### Recommended Production (Medium Scale)
- **CPU:** 4-8 cores
- **RAM:** 8-16GB
- **Storage:** 100-500GB NVMe SSD
- **Network:** 1 Gbps
- **Expected Load:** ~50K writes/sec, ~25K queries/sec

#### High Scale Production
- **CPU:** 16+ cores
- **RAM:** 32+ GB
- **Storage:** 1TB+ NVMe SSD
- **Network:** 10+ Gbps
- **Expected Load:** ~200K+ writes/sec, ~100K+ queries/sec

### Storage Considerations

**Storage Types:**
- **NVMe SSD:** Best performance for high-write workloads
- **SATA SSD:** Good balance of performance and cost
- **HDD:** Only for development/testing or cold storage

**Storage Planning:**
```
Data Size Estimation:
- Raw data point: ~24 bytes (timestamp + value + metadata)
- Compressed: ~2-4 bytes per point (varies by pattern)
- Index overhead: ~10-20% additional space
- Replication: multiply by replication factor

Example: 1M points/day × 365 days × 3 bytes × 2 replicas = ~2.2GB/year
```

**Storage Layout:**
```
/data/count/
├── node_state/        # Cluster membership info
├── series_index/      # Series metadata and indexes
├── time_blocks/       # Time-partitioned data blocks
│   ├── 2024-01/
│   ├── 2024-02/
│   └── ...
└── wal/              # Write-ahead log (future)
```

### Network Requirements

**Bandwidth Planning:**
- **Client traffic:** Proportional to query/write volume
- **Replication traffic:** ~2x write bandwidth
- **Gossip protocol:** Minimal (<1Mbps per node)
- **Query distribution:** Variable based on query patterns

**Latency Requirements:**
- **Same datacenter:** <1ms preferred, <5ms acceptable
- **Cross-AZ:** <10ms acceptable
- **Cross-region:** Not recommended for cluster nodes

## Deployment Patterns

### Single Datacenter Deployment

**Architecture:**
```
Load Balancer
     │
     ▼
┌─────────┐  ┌─────────┐  ┌─────────┐
│ Count-1 │  │ Count-2 │  │ Count-3 │
│  (AZ-A) │  │  (AZ-B) │  │  (AZ-C) │
└─────────┘  └─────────┘  └─────────┘
     │            │            │
     └────────────┼────────────┘
                  │
              Shared Network
```

**Benefits:**
- Low latency between nodes
- Simple network configuration
- Consistent performance

**Considerations:**
- Single point of failure (datacenter)
- Limited geographic distribution

### Multi-AZ Deployment

**Architecture:**
```
         Load Balancer
              │
    ┌─────────┼─────────┐
    ▼         ▼         ▼
┌─────────┐ ┌─────────┐ ┌─────────┐
│ Count-1 │ │ Count-2 │ │ Count-3 │
│  AZ-1a  │ │  AZ-1b  │ │  AZ-1c  │
└─────────┘ └─────────┘ └─────────┘
     │           │           │
     └───────────┼───────────┘
                 │
         Cross-AZ Network
```

**Configuration:**
```yaml
# docker-compose.multi-az.yml
version: '3.8'
services:
  count-node-1:
    deploy:
      placement:
        constraints: [node.labels.zone == az-1a]
    environment:
      - NODE_ID=1
      - CLUSTER_NODES=count-1.az1a:8080,count-2.az1b:8080,count-3.az1c:8080
      
  count-node-2:
    deploy:
      placement:
        constraints: [node.labels.zone == az-1b]
    environment:
      - NODE_ID=2
      - CLUSTER_NODES=count-1.az1a:8080,count-2.az1b:8080,count-3.az1c:8080
```

### Kubernetes Deployment

#### Namespace and RBAC

```yaml
# namespace.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: count-system
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: count
  namespace: count-system
```

#### ConfigMap

```yaml
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: count-config
  namespace: count-system
data:
  CLUSTER_ENABLED: "true"
  REPLICATION_FACTOR: "2"
  BIND_ADDRESS: "0.0.0.0:8080"
  DATA_DIR: "/data"
  MEMORY_BUFFER_SIZE: "25000"
  FLUSH_INTERVAL_SECONDS: "300"
  RUST_LOG: "info"
```

#### StatefulSet

```yaml
# statefulset.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: count
  namespace: count-system
spec:
  serviceName: count
  replicas: 3
  selector:
    matchLabels:
      app: count
  template:
    metadata:
      labels:
        app: count
    spec:
      serviceAccountName: count
      containers:
      - name: count
        image: count:v0.1.0
        ports:
        - containerPort: 8080
          name: http
        env:
        - name: NODE_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: CLUSTER_NODES
          value: "count-0.count.count-system.svc.cluster.local:8080,count-1.count.count-system.svc.cluster.local:8080,count-2.count.count-system.svc.cluster.local:8080"
        envFrom:
        - configMapRef:
            name: count-config
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
        resources:
          requests:
            cpu: 100m
            memory: 256Mi
          limits:
            cpu: 2
            memory: 4Gi
        volumeMounts:
        - name: data
          mountPath: /data
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 100Gi
```

#### Services

```yaml
# services.yaml
apiVersion: v1
kind: Service
metadata:
  name: count
  namespace: count-system
spec:
  clusterIP: None
  selector:
    app: count
  ports:
  - port: 8080
    name: http
---
apiVersion: v1
kind: Service
metadata:
  name: count-external
  namespace: count-system
spec:
  type: LoadBalancer
  selector:
    app: count
  ports:
  - port: 80
    targetPort: 8080
    name: http
```

#### Ingress

```yaml
# ingress.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: count-ingress
  namespace: count-system
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
spec:
  tls:
  - hosts:
    - count.yourdomain.com
    secretName: count-tls
  rules:
  - host: count.yourdomain.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: count-external
            port:
              number: 80
```

### Docker Swarm Deployment

```yaml
# docker-stack.yml
version: '3.8'

services:
  count:
    image: count:v0.1.0
    deploy:
      replicas: 3
      placement:
        max_replicas_per_node: 1
      update_config:
        parallelism: 1
        delay: 30s
      restart_policy:
        condition: on-failure
        delay: 5s
        max_attempts: 3
    environment:
      - CLUSTER_ENABLED=true
      - REPLICATION_FACTOR=2
      - BIND_ADDRESS=0.0.0.0:8080
      - DATA_DIR=/data
      - MEMORY_BUFFER_SIZE=25000
      - FLUSH_INTERVAL_SECONDS=300
      - RUST_LOG=info
    volumes:
      - count-data:/data
    networks:
      - count-net
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
      - ./ssl:/etc/ssl
    networks:
      - count-net
    depends_on:
      - count

volumes:
  count-data:
    driver: local

networks:
  count-net:
    driver: overlay
    attachable: true
```

## Load Balancing

### Nginx Configuration

```nginx
# nginx.conf
upstream count_cluster {
    least_conn;
    server count-1:8080 max_fails=3 fail_timeout=30s;
    server count-2:8080 max_fails=3 fail_timeout=30s;
    server count-3:8080 max_fails=3 fail_timeout=30s;
}

server {
    listen 80;
    server_name count.yourdomain.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name count.yourdomain.com;
    
    ssl_certificate /etc/ssl/certs/count.crt;
    ssl_certificate_key /etc/ssl/private/count.key;
    
    # SSL Configuration
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-RSA-AES256-GCM-SHA512:DHE-RSA-AES256-GCM-SHA512;
    ssl_prefer_server_ciphers off;
    
    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api:10m rate=100r/s;
    
    location /health {
        proxy_pass http://count_cluster;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_connect_timeout 5s;
        proxy_read_timeout 10s;
    }
    
    location /insert {
        limit_req zone=api burst=20 nodelay;
        proxy_pass http://count_cluster;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_connect_timeout 5s;
        proxy_read_timeout 30s;
    }
    
    location /query/ {
        proxy_pass http://count_cluster;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_connect_timeout 5s;
        proxy_read_timeout 60s;
    }
    
    # Block internal cluster endpoints
    location ~ ^/(cluster|replicate) {
        deny all;
        return 403;
    }
}
```

### HAProxy Configuration

```
# haproxy.cfg
global
    daemon
    maxconn 4096
    
defaults
    mode http
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms
    
frontend count_frontend
    bind *:80
    bind *:443 ssl crt /etc/ssl/certs/count.pem
    redirect scheme https if !{ ssl_fc }
    default_backend count_cluster
    
backend count_cluster
    balance roundrobin
    option httpchk GET /health
    server count-1 count-1:8080 check
    server count-2 count-2:8080 check  
    server count-3 count-3:8080 check
```

## Security Hardening

### Network Security

**Firewall Rules (iptables):**
```bash
#!/bin/bash
# secure-count.sh

# Drop all by default
iptables -P INPUT DROP
iptables -P FORWARD DROP
iptables -P OUTPUT ACCEPT

# Allow loopback
iptables -A INPUT -i lo -j ACCEPT

# Allow established connections
iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT

# Allow SSH (adjust source as needed)
iptables -A INPUT -p tcp --dport 22 -s 10.0.0.0/8 -j ACCEPT

# Allow Count API from load balancer
iptables -A INPUT -p tcp --dport 8080 -s 10.0.1.10 -j ACCEPT  # LB IP

# Allow inter-cluster communication
iptables -A INPUT -p tcp --dport 8080 -s 10.0.2.10 -j ACCEPT  # Node 1
iptables -A INPUT -p tcp --dport 8080 -s 10.0.2.11 -j ACCEPT  # Node 2
iptables -A INPUT -p tcp --dport 8080 -s 10.0.2.12 -j ACCEPT  # Node 3

# Save rules
iptables-save > /etc/iptables/rules.v4
```

**AWS Security Groups:**
```json
{
  "SecurityGroups": [
    {
      "GroupName": "count-cluster",
      "Description": "Count database cluster",
      "SecurityGroupRules": [
        {
          "IpProtocol": "tcp",
          "FromPort": 8080,
          "ToPort": 8080,
          "SourceSecurityGroupId": "sg-loadbalancer"
        },
        {
          "IpProtocol": "tcp", 
          "FromPort": 8080,
          "ToPort": 8080,
          "SourceSecurityGroupId": "sg-count-cluster"
        }
      ]
    }
  ]
}
```

### Container Security

**Dockerfile security best practices:**
```dockerfile
FROM rust:1.75-slim as builder
# ... build steps ...

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user
RUN groupadd -r count && useradd -r -g count count

# Set up directories with proper permissions
RUN mkdir -p /data && chown count:count /data
RUN mkdir -p /logs && chown count:count /logs

# Copy binary
COPY --from=builder /usr/src/app/target/release/count-server /usr/local/bin/count-server
RUN chmod +x /usr/local/bin/count-server

# Drop privileges
USER count
WORKDIR /data

# Security labels
LABEL security.non-root=true
LABEL security.no-new-privileges=true

CMD ["count-server"]
```

**Docker Compose security:**
```yaml
services:
  count-node-1:
    image: count:latest
    user: 1000:1000  # Run as specific UID/GID
    read_only: true   # Read-only root filesystem
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    tmpfs:
      - /tmp:noexec,nosuid,size=100m
    volumes:
      - count-data-1:/data:rw
      - count-logs-1:/logs:rw
```

## Monitoring and Observability

### Health Check Endpoints

**External monitoring:**
```bash
#!/bin/bash
# monitor-count.sh

NODES=("count-1:8080" "count-2:8080" "count-3:8080")
ALERT_EMAIL="ops@yourdomain.com"

for node in "${NODES[@]}"; do
    response=$(curl -s -w "%{http_code}" -o /tmp/health.json http://$node/health)
    
    if [ "$response" != "200" ]; then
        echo "ALERT: Node $node is unhealthy (HTTP $response)" | \
            mail -s "Count Node Alert" $ALERT_EMAIL
    else
        cluster_size=$(jq -r '.cluster_size' /tmp/health.json)
        if [ "$cluster_size" -lt 3 ]; then
            echo "WARNING: Cluster size reduced to $cluster_size" | \
                mail -s "Count Cluster Warning" $ALERT_EMAIL
        fi
    fi
done
```

### Log Management

**Centralized logging with Fluent Bit:**
```yaml
# fluent-bit.yml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fluent-bit-config
data:
  fluent-bit.conf: |
    [SERVICE]
        Flush         1
        Log_Level     info
        Daemon        off
        
    [INPUT]
        Name              tail
        Path              /var/log/count/*.log
        Tag               count.*
        Refresh_Interval  5
        
    [OUTPUT]
        Name  es
        Match count.*
        Host  elasticsearch.logging
        Port  9200
        Index count-logs
```

### Metrics Collection

**Prometheus monitoring (future enhancement):**
```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
- job_name: 'count'
  static_configs:
  - targets: 
    - 'count-1:8080'
    - 'count-2:8080' 
    - 'count-3:8080'
  metrics_path: '/metrics'  # Future endpoint
```

## Backup and Disaster Recovery

### Backup Strategy

**Volume-based backups:**
```bash
#!/bin/bash
# backup-count.sh

BACKUP_DIR="/backups/count"
DATE=$(date +%Y-%m-%d-%H-%M-%S)

# Create backup directory
mkdir -p "$BACKUP_DIR/$DATE"

# Stop Count (optional, for consistent backup)
docker-compose stop count-node-1 count-node-2 count-node-3

# Backup data volumes
docker run --rm \
    -v count_count-data-1:/source:ro \
    -v "$BACKUP_DIR/$DATE":/backup \
    alpine tar czf /backup/node-1-data.tar.gz -C /source .

docker run --rm \
    -v count_count-data-2:/source:ro \
    -v "$BACKUP_DIR/$DATE":/backup \
    alpine tar czf /backup/node-2-data.tar.gz -C /source .

docker run --rm \
    -v count_count-data-3:/source:ro \
    -v "$BACKUP_DIR/$DATE":/backup \
    alpine tar czf /backup/node-3-data.tar.gz -C /source .

# Restart Count
docker-compose start count-node-1 count-node-2 count-node-3

# Upload to S3 (optional)
aws s3 sync "$BACKUP_DIR/$DATE" s3://your-backup-bucket/count/$DATE/

echo "Backup completed: $BACKUP_DIR/$DATE"
```

**Kubernetes backup with Velero:**
```bash
# Install Velero
velero install --provider aws --bucket velero-backups

# Backup Count namespace
velero backup create count-backup --include-namespaces count-system

# Schedule regular backups
velero schedule create count-daily --schedule="0 2 * * *" --include-namespaces count-system
```

### Disaster Recovery

**Recovery procedure:**
```bash
#!/bin/bash
# restore-count.sh

BACKUP_DIR="/backups/count/2024-01-15-02-00-00"

# Stop existing cluster
docker-compose down

# Remove old volumes
docker volume rm count_count-data-1 count_count-data-2 count_count-data-3

# Restore data volumes
docker run --rm \
    -v count_count-data-1:/target \
    -v "$BACKUP_DIR":/backup:ro \
    alpine tar xzf /backup/node-1-data.tar.gz -C /target

docker run --rm \
    -v count_count-data-2:/target \
    -v "$BACKUP_DIR":/backup:ro \
    alpine tar xzf /backup/node-2-data.tar.gz -C /target

docker run --rm \
    -v count_count-data-3:/target \
    -v "$BACKUP_DIR":/backup:ro \
    alpine tar xzf /backup/node-3-data.tar.gz -C /target

# Start cluster
docker-compose up -d

# Verify recovery
sleep 30
curl http://localhost:8081/health
```

## Performance Tuning

### OS-Level Tuning

**Linux system tuning:**
```bash
#!/bin/bash
# tune-system.sh

# Increase file descriptor limits
echo "count soft nofile 65536" >> /etc/security/limits.conf
echo "count hard nofile 65536" >> /etc/security/limits.conf

# TCP tuning for high-throughput
echo 'net.core.rmem_max = 134217728' >> /etc/sysctl.conf
echo 'net.core.wmem_max = 134217728' >> /etc/sysctl.conf
echo 'net.ipv4.tcp_rmem = 4096 65536 134217728' >> /etc/sysctl.conf
echo 'net.ipv4.tcp_wmem = 4096 65536 134217728' >> /etc/sysctl.conf

# Apply settings
sysctl -p
```

### Storage Tuning

**XFS tuning for time series workloads:**
```bash
# Mount with optimized options
mount -t xfs -o noatime,largeio,inode64,swalloc /dev/nvme0n1 /data/count
```

**Docker volume optimization:**
```yaml
volumes:
  count-data-1:
    driver: local
    driver_opts:
      type: xfs
      device: /dev/nvme0n1p1
      o: "noatime,largeio"
```

This deployment guide provides the foundation for running Count in production environments. Adapt the configurations to your specific infrastructure and requirements.