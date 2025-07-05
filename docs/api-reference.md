# API Reference

Count provides a comprehensive HTTP REST API for all database operations. This document covers all endpoints, request/response formats, and usage examples.

## Base URL and Versioning

```
Base URL: http://<node-address>:8080
API Version: v1 (implicit, no version prefix required)
Content-Type: application/json (for requests with body)
```

## Authentication

Count currently does not implement built-in authentication. Access control should be implemented at the network level (firewalls, VPN, load balancer authentication, etc.).

## Common Response Codes

| Code | Meaning | Description |
|------|---------|-------------|
| 200 | OK | Request successful |
| 201 | Created | Resource created successfully |
| 400 | Bad Request | Invalid request format or parameters |
| 404 | Not Found | Series or resource not found |
| 500 | Internal Server Error | Server error occurred |

## Data Models

### DataPoint

Represents a single time series measurement:

```json
{
  "timestamp": 1640995200000,  // Unix timestamp in milliseconds
  "value": 42.5               // Numeric value (f64)
}
```

### Series Key

Time series are identified by string keys:
- Can contain letters, numbers, dots, underscores, hyphens
- Recommended format: `<namespace>.<metric>.<tags>`
- Examples: `cpu.usage.total`, `server1.memory.percent`, `api.latency.p99`

## Health and Status Endpoints

### GET /health

Check node and cluster health status.

**Request:**
```http
GET /health
```

**Response:**
```json
{
  "status": "healthy",     // "healthy" | "degraded" | "unhealthy"
  "node_id": 1,           // This node's ID
  "cluster_size": 3       // Number of healthy nodes in cluster
}
```

**Example:**
```bash
curl http://localhost:8080/health
```

## Data Ingestion

### POST /insert

Insert a single data point into a time series.

**Request:**
```http
POST /insert
Content-Type: application/json

{
  "series": "cpu.usage.total",
  "timestamp": 1640995200000,
  "value": 75.5
}
```

**Response:**
```http
HTTP/1.1 201 Created
```

**Parameters:**
- `series` (string, required): Time series identifier
- `timestamp` (integer, required): Unix timestamp in milliseconds
- `value` (number, required): Numeric value to store

**Examples:**

```bash
# Insert CPU usage
curl -X POST http://localhost:8080/insert \
  -H "Content-Type: application/json" \
  -d '{
    "series": "cpu.usage.total",
    "timestamp": 1640995200000,
    "value": 75.5
  }'

# Insert memory usage
curl -X POST http://localhost:8080/insert \
  -H "Content-Type: application/json" \
  -d '{
    "series": "memory.usage.percent",
    "timestamp": 1640995200000,
    "value": 60.2
  }'

# Insert with current timestamp
timestamp=$(date +%s000)
curl -X POST http://localhost:8080/insert \
  -H "Content-Type: application/json" \
  -d "{
    \"series\": \"temperature.sensor1\",
    \"timestamp\": $timestamp,
    \"value\": 23.5
  }"
```

**Error Responses:**

```json
// 400 Bad Request - Invalid data
{
  "error": "Invalid timestamp: must be positive integer"
}

// 500 Internal Server Error - Storage failure
{
  "error": "Failed to write data: disk full"
}
```

## Data Querying

### GET /query/range/{series}

Retrieve all data points for a series within a time range.

**Request:**
```http
GET /query/range/{series}?start={timestamp}&end={timestamp}
```

**Parameters:**
- `series` (path, required): Time series identifier
- `start` (query, required): Start timestamp (milliseconds, inclusive)
- `end` (query, required): End timestamp (milliseconds, inclusive)

**Response:**
```json
[
  {
    "timestamp": 1640995200000,
    "value": 75.5
  },
  {
    "timestamp": 1640995260000,
    "value": 82.1
  }
]
```

**Examples:**

```bash
# Query CPU data for last hour
start_time=$(($(date +%s000) - 3600000))  # 1 hour ago
end_time=$(date +%s000)                   # Now
curl "http://localhost:8080/query/range/cpu.usage.total?start=$start_time&end=$end_time"

# Query specific time range
curl "http://localhost:8080/query/range/memory.usage.percent?start=1640995000000&end=1640998600000"

# Query with URL encoding for complex series names
series="server-1.disk.usage./var/log"
encoded_series=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$series'))")
curl "http://localhost:8080/query/range/$encoded_series?start=1640995000000&end=1640998600000"
```

### GET /query/aggregated/{series}

Compute aggregations over a time range.

**Request:**
```http
GET /query/aggregated/{series}?start={timestamp}&end={timestamp}&aggregation={type}
```

**Parameters:**
- `series` (path, required): Time series identifier
- `start` (query, required): Start timestamp (milliseconds, inclusive)
- `end` (query, required): End timestamp (milliseconds, inclusive)
- `aggregation` (query, required): Aggregation type

**Aggregation Types:**
- `sum`: Sum of all values
- `mean` or `avg`: Average of all values
- `min`: Minimum value
- `max`: Maximum value
- `count`: Count of data points

**Response:**
```json
42.75  // Single numeric value
```

**Examples:**

```bash
# Average CPU usage over last hour
start_time=$(($(date +%s000) - 3600000))
end_time=$(date +%s000)
curl "http://localhost:8080/query/aggregated/cpu.usage.total?start=$start_time&end=$end_time&aggregation=mean"

# Maximum memory usage
curl "http://localhost:8080/query/aggregated/memory.usage.percent?start=1640995000000&end=1640998600000&aggregation=max"

# Total requests count
curl "http://localhost:8080/query/aggregated/api.requests.count?start=1640995000000&end=1640998600000&aggregation=sum"

# Count of data points
curl "http://localhost:8080/query/aggregated/sensor.temperature?start=1640995000000&end=1640998600000&aggregation=count"
```

## Internal Cluster Endpoints

These endpoints are used for inter-node communication in clustered deployments.

### POST /cluster/join

Handle node joining requests (used internally by gossip protocol).

**Request:**
```json
{
  "sender_id": 2,
  "message_type": {
    "NodeJoin": {
      "node": {
        "id": 2,
        "address": "192.168.1.11:8080",
        "status": "Joining",
        "last_seen": 1640995200000,
        "version": "0.1.0",
        "data_size": 0
      }
    }
  },
  "timestamp": 1640995200000
}
```

### POST /cluster/gossip

Handle gossip protocol messages (used internally).

**Message Types:**
- `Heartbeat`: Node health updates
- `NodeJoin`: New nodes joining
- `NodeLeave`: Graceful departures
- `NodeFailed`: Failure notifications
- `ClusterState`: Full cluster membership

### POST /replicate

Handle data replication between nodes (used internally).

**Request:**
```json
{
  "series": "cpu.usage.total",
  "points": [
    {
      "timestamp": 1640995200000,
      "value": 75.5
    }
  ],
  "replication_id": "uuid-string",
  "source_node": 1
}
```

### POST /query/range (Internal)

Handle distributed query requests between nodes.

### POST /query/aggregated (Internal)

Handle distributed aggregation requests between nodes.

## Batch Operations

Currently, Count supports single-point inserts. Batch operations can be implemented at the client level:

```bash
#!/bin/bash
# Batch insert example

data_points=(
  '{"series": "cpu.usage", "timestamp": 1640995200000, "value": 75.5}'
  '{"series": "cpu.usage", "timestamp": 1640995260000, "value": 82.1}'
  '{"series": "cpu.usage", "timestamp": 1640995320000, "value": 69.3}'
)

for point in "${data_points[@]}"; do
  curl -X POST http://localhost:8080/insert \
    -H "Content-Type: application/json" \
    -d "$point" &  # Background for parallel execution
done

wait  # Wait for all requests to complete
echo "Batch insert completed"
```

## Error Handling

### Error Response Format

All errors return JSON with an error message:

```json
{
  "error": "Descriptive error message"
}
```

### Common Error Scenarios

**1. Invalid Series Name:**
```bash
curl -X POST http://localhost:8080/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "", "timestamp": 1640995200000, "value": 42}'

# Response: 400 Bad Request
# {"error": "Series name cannot be empty"}
```

**2. Invalid Timestamp:**
```bash
curl -X POST http://localhost:8080/insert \
  -H "Content-Type: application/json" \
  -d '{"series": "test", "timestamp": -1, "value": 42}'

# Response: 400 Bad Request
# {"error": "Invalid timestamp: must be positive integer"}
```

**3. Invalid Aggregation Type:**
```bash
curl "http://localhost:8080/query/aggregated/test.series?start=0&end=999999999999&aggregation=invalid"

# Response: 400 Bad Request
# {"error": "Invalid aggregation type"}
```

**4. Series Not Found:**
```bash
curl "http://localhost:8080/query/range/nonexistent.series?start=0&end=999999999999"

# Response: 200 OK (returns empty array)
# []
```

**5. Internal Server Error:**
```bash
# If storage fails
# Response: 500 Internal Server Error
# {"error": "Failed to write data: disk full"}
```

## Rate Limiting and Throttling

Count does not implement built-in rate limiting. Implement rate limiting at the application or infrastructure level:

**Nginx example:**
```nginx
http {
  limit_req_zone $binary_remote_addr zone=api:10m rate=100r/s;
  
  server {
    location /insert {
      limit_req zone=api burst=20 nodelay;
      proxy_pass http://count-cluster;
    }
  }
}
```

## Performance Considerations

### Query Optimization

**1. Time Range Queries:**
```bash
# Efficient: Specific time range
curl "http://localhost:8080/query/range/cpu.usage?start=1640995000000&end=1640995300000"

# Inefficient: Very large time range
curl "http://localhost:8080/query/range/cpu.usage?start=0&end=999999999999999"
```

**2. Aggregation vs Range:**
```bash
# Efficient: Use aggregation when you don't need raw data
curl "http://localhost:8080/query/aggregated/cpu.usage?start=1640995000000&end=1640995300000&aggregation=mean"

# Less efficient: Fetching all points just to calculate average
curl "http://localhost:8080/query/range/cpu.usage?start=1640995000000&end=1640995300000" | jq 'map(.value) | add / length'
```

### Insertion Patterns

**1. Batch Insertions:**
```bash
# Better: Parallel insertions
for i in {1..100}; do
  curl -X POST http://localhost:8080/insert -d "{...}" &
done
wait

# Avoid: Sequential insertions for high throughput
for i in {1..100}; do
  curl -X POST http://localhost:8080/insert -d "{...}"
done
```

**2. Time Ordering:**
Insert data points in roughly chronological order for better performance.

## Client Libraries and SDKs

### cURL Examples Collection

**Complete workflow example:**
```bash
#!/bin/bash
set -e

BASE_URL="http://localhost:8080"
SERIES="example.workflow"
START_TIME=$(date +%s000)

echo "1. Inserting sample data..."
for i in {0..59}; do
  timestamp=$((START_TIME + i * 60000))  # Every minute
  value=$((20 + RANDOM % 60))           # Random value 20-80
  
  curl -s -X POST "$BASE_URL/insert" \
    -H "Content-Type: application/json" \
    -d "{\"series\": \"$SERIES\", \"timestamp\": $timestamp, \"value\": $value}" \
    > /dev/null
    
  if [ $((i % 10)) -eq 0 ]; then
    echo "  Inserted $i points..."
  fi
done

echo "2. Querying range data..."
END_TIME=$((START_TIME + 3600000))  # 1 hour later
RANGE_RESULT=$(curl -s "$BASE_URL/query/range/$SERIES?start=$START_TIME&end=$END_TIME")
POINT_COUNT=$(echo "$RANGE_RESULT" | jq length)
echo "  Found $POINT_COUNT data points"

echo "3. Computing aggregations..."
MEAN=$(curl -s "$BASE_URL/query/aggregated/$SERIES?start=$START_TIME&end=$END_TIME&aggregation=mean")
MAX=$(curl -s "$BASE_URL/query/aggregated/$SERIES?start=$START_TIME&end=$END_TIME&aggregation=max")
MIN=$(curl -s "$BASE_URL/query/aggregated/$SERIES?start=$START_TIME&end=$END_TIME&aggregation=min")

echo "  Mean: $MEAN"
echo "  Max: $MAX"
echo "  Min: $MIN"

echo "4. Checking cluster health..."
HEALTH=$(curl -s "$BASE_URL/health")
echo "  $HEALTH"

echo "Workflow completed successfully!"
```

### Python Client Example

```python
import requests
import json
import time

class CountClient:
    def __init__(self, base_url="http://localhost:8080"):
        self.base_url = base_url
        self.session = requests.Session()
    
    def insert(self, series, timestamp, value):
        """Insert a data point"""
        data = {
            "series": series,
            "timestamp": timestamp,
            "value": value
        }
        response = self.session.post(f"{self.base_url}/insert", json=data)
        response.raise_for_status()
        return response.status_code == 201
    
    def query_range(self, series, start, end):
        """Query data points in range"""
        params = {"start": start, "end": end}
        response = self.session.get(f"{self.base_url}/query/range/{series}", params=params)
        response.raise_for_status()
        return response.json()
    
    def query_aggregated(self, series, start, end, aggregation):
        """Query aggregated value"""
        params = {"start": start, "end": end, "aggregation": aggregation}
        response = self.session.get(f"{self.base_url}/query/aggregated/{series}", params=params)
        response.raise_for_status()
        return response.json()
    
    def health(self):
        """Check health status"""
        response = self.session.get(f"{self.base_url}/health")
        response.raise_for_status()
        return response.json()

# Usage example
client = CountClient()

# Insert data
now = int(time.time() * 1000)
client.insert("python.example", now, 42.5)

# Query data
points = client.query_range("python.example", now - 3600000, now)
print(f"Found {len(points)} points")

# Get average
avg = client.query_aggregated("python.example", now - 3600000, now, "mean")
print(f"Average: {avg}")

# Check health
health = client.health()
print(f"Cluster status: {health}")
```

### JavaScript Client Example

```javascript
class CountClient {
  constructor(baseUrl = 'http://localhost:8080') {
    this.baseUrl = baseUrl;
  }

  async insert(series, timestamp, value) {
    const response = await fetch(`${this.baseUrl}/insert`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ series, timestamp, value })
    });
    
    if (!response.ok) {
      throw new Error(`Insert failed: ${response.statusText}`);
    }
    
    return response.status === 201;
  }

  async queryRange(series, start, end) {
    const url = `${this.baseUrl}/query/range/${series}?start=${start}&end=${end}`;
    const response = await fetch(url);
    
    if (!response.ok) {
      throw new Error(`Query failed: ${response.statusText}`);
    }
    
    return await response.json();
  }

  async queryAggregated(series, start, end, aggregation) {
    const url = `${this.baseUrl}/query/aggregated/${series}?start=${start}&end=${end}&aggregation=${aggregation}`;
    const response = await fetch(url);
    
    if (!response.ok) {
      throw new Error(`Query failed: ${response.statusText}`);
    }
    
    return await response.json();
  }

  async health() {
    const response = await fetch(`${this.baseUrl}/health`);
    
    if (!response.ok) {
      throw new Error(`Health check failed: ${response.statusText}`);
    }
    
    return await response.json();
  }
}

// Usage
const client = new CountClient();

// Insert data
const now = Date.now();
await client.insert('js.example', now, 75.5);

// Query data
const points = await client.queryRange('js.example', now - 3600000, now);
console.log(`Found ${points.length} points`);

// Get statistics
const avg = await client.queryAggregated('js.example', now - 3600000, now, 'mean');
console.log(`Average: ${avg}`);
```

This API reference provides everything needed to integrate with Count. For more advanced usage patterns and deployment scenarios, see the other documentation sections.