# S3 Integration for Count Time Series Database

This document describes the S3 integration feature that provides automatic archival of older time series data to Amazon S3.

## Overview

The S3 integration implements a tiered storage architecture:

- **Hot Storage (Memory)**: Most recent data points for fast access
- **Warm Storage (Local Disk)**: Recent historical data (< 2 weeks by default)  
- **Cold Storage (S3)**: Archived data older than 2 weeks

## Configuration

### Environment Variables

Set these environment variables to enable S3 integration:

```bash
# Required
S3_ENABLED=true
S3_BUCKET=your-bucket-name

# Optional
S3_REGION=us-east-1              # Defaults to AWS default region
S3_PREFIX=count-data             # S3 key prefix, defaults to "count-data"
ARCHIVAL_AGE_DAYS=14             # Days before archiving to S3, defaults to 14

# Standard configuration
DATA_DIR=./count_data            # Local data directory
MEMORY_BUFFER_SIZE=10000         # Points to keep in memory per series
```

### Programmatic Configuration

```rust
use count::{CountConfig, CountDB};

let mut config = CountConfig::default();
config.s3_enabled = true;
config.s3_bucket = Some("my-timeseries-bucket".to_string());
config.s3_region = Some("us-west-2".to_string());
config.s3_prefix = Some("count-data".to_string());
config.archival_age_days = 14;

let db = CountDB::new(config).await?;
```

## Features

### Automatic Archival

- Background process runs every hour
- Automatically migrates data older than `archival_age_days` from local disk to S3
- Compressed blocks are stored efficiently in S3
- Original disk data is cleaned up after successful migration

### Transparent Queries

Queries automatically check all storage tiers:

```rust
// This query will check memory, disk, and S3 as needed
let points = db.query_range(
    SeriesKey::from("cpu.usage"),
    start_time,
    end_time
).await?;
```

Query execution order:
1. **Memory buffer** - fastest, most recent data
2. **Local disk** - warm data from recent weeks  
3. **S3 storage** - cold archived data for historical queries

### Manual Operations

Force immediate archival:
```rust
let storage = // get storage engine reference
let archived_count = storage.force_archival().await?;
println!("Archived {} blocks to S3", archived_count);
```

## S3 Storage Format

### Directory Structure
```
s3://your-bucket/count-data/
├── series1/
│   ├── manifest.json
│   ├── block_1000_2000.json
│   └── block_2000_3000.json
└── series2/
    ├── manifest.json
    └── block_1500_2500.json
```

### Block Format
Each block contains compressed time series data:
- Timestamp compression using delta-of-delta encoding
- Value compression using XOR-based algorithms
- Metadata including time ranges and point counts

### Manifest Format
```json
{
  "series_name": "cpu.usage",
  "blocks": [
    {
      "key": "count-data/cpu.usage/block_1000_2000.json",
      "start_time": 1000,
      "end_time": 2000,
      "point_count": 1000
    }
  ]
}
```

## Performance Characteristics

### Storage Efficiency
- Compressed blocks achieve high compression ratios for time series data
- S3 storage is cost-effective for infrequently accessed historical data
- Local storage used only for recent, frequently queried data

### Query Performance  
- Memory queries: < 1ms latency
- Disk queries: 1-10ms latency  
- S3 queries: 100-500ms latency (depending on data size and network)

### Write Performance
- Writes go to memory buffer first (microsecond latency)
- Background flush to disk every 5 minutes
- Background archival to S3 every hour
- No impact on write path performance

## AWS IAM Permissions

The application needs these S3 permissions:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject", 
        "s3:DeleteObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::your-bucket-name/*",
        "arn:aws:s3:::your-bucket-name"
      ]
    }
  ]
}
```

## Cost Optimization

- Use S3 Intelligent Tiering for automatic cost optimization
- Consider S3 lifecycle policies for very old data
- Monitor S3 costs and adjust archival age as needed

```bash
# Example: Archive after 30 days instead of 14 to reduce S3 costs
ARCHIVAL_AGE_DAYS=30
```

## Monitoring

The system logs archival operations:
- Successful archival: "Successfully completed S3 archival for data older than X days"
- Archival errors: "Error during S3 archival: [error details]"

Monitor S3 costs and access patterns through AWS CloudWatch and Cost Explorer.

## Troubleshooting

### Common Issues

**S3 authentication errors**
- Ensure AWS credentials are configured (AWS CLI, IAM roles, or environment variables)
- Verify IAM permissions for the S3 bucket

**Network errors during archival**
- Check network connectivity to S3
- Consider increasing retry limits for transient failures

**High S3 costs**
- Increase `ARCHIVAL_AGE_DAYS` to keep more data local
- Enable S3 Intelligent Tiering
- Review query patterns - frequent historical queries may warrant keeping data local longer

### Debug Mode

Enable debug logging to see detailed S3 operations:
```bash
RUST_LOG=debug ./count-server
```

## Migration Guide

### Enabling S3 on Existing Database

1. **Backup existing data** (recommended)
2. **Set S3 configuration** environment variables  
3. **Restart the application**
4. **Monitor initial archival** - existing old data will be migrated on first run

### Disabling S3 Integration

1. Set `S3_ENABLED=false`
2. Restart application
3. Data in S3 will remain but won't be queried
4. To retrieve S3 data, temporarily re-enable S3 integration

## Examples

See `examples/s3_integration.rs` for a complete example of configuring and using S3 integration.