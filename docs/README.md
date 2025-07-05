# Count Documentation

Welcome to the Count time series database documentation. Count is a high-performance, distributed time series database built in Rust with advanced clustering and sharding capabilities.

## Documentation Structure

- [Architecture Overview](architecture.md) - System design and components
- [Getting Started](getting-started.md) - Quick start guide and basic usage
- [Clustering Guide](clustering.md) - Multi-node setup and distributed operations
- [API Reference](api-reference.md) - Complete HTTP API documentation
- [Configuration](configuration.md) - Configuration options and environment variables
- [Deployment](deployment.md) - Production deployment guides
- [Operations](operations.md) - Monitoring, scaling, and maintenance
- [Development](development.md) - Contributing and development setup
- [Troubleshooting](troubleshooting.md) - Common issues and solutions

## Quick Links

### For Users
- [Quick Start with Docker](getting-started.md#docker-quickstart)
- [Single Node Setup](getting-started.md#single-node-setup)
- [Multi-Node Cluster](clustering.md#setting-up-a-cluster)
- [API Usage Examples](api-reference.md#examples)

### For Operators
- [Production Deployment](deployment.md#production-deployment)
- [Monitoring Setup](operations.md#monitoring)
- [Backup and Recovery](operations.md#backup-and-recovery)
- [Scaling Operations](operations.md#scaling)

### For Developers
- [Development Setup](development.md#development-environment)
- [Architecture Deep Dive](architecture.md#detailed-architecture)
- [Contributing Guidelines](development.md#contributing)
- [Running Tests](development.md#testing)

## What is Count?

Count is a distributed time series database designed for:
- **High Performance**: Optimized for time series workloads with efficient compression
- **Scalability**: Linear scaling with consistent hash-based sharding
- **Reliability**: Multi-replica fault tolerance with automatic recovery
- **Simplicity**: Easy deployment with Docker and straightforward API

## Key Features

### Core Database
- Time-series optimized storage engine
- Configurable memory buffering and disk persistence
- Efficient compression algorithms
- Range queries and aggregations (sum, mean, min, max, count)

### Distributed Systems
- Consistent hash-based automatic sharding
- Configurable replication factor
- Gossip-based node discovery and failure detection
- Distributed query processing with automatic routing
- Linear horizontal scaling

### Operations
- Docker containerization with Docker Compose orchestration
- REST HTTP API for all operations
- Health monitoring and cluster status endpoints
- Automatic data rebalancing on topology changes
- Hot-swappable node addition and removal

## Use Cases

Count is ideal for:
- **Infrastructure Monitoring**: CPU, memory, disk, network metrics
- **Application Performance Monitoring**: Response times, throughput, error rates
- **IoT Data Collection**: Sensor readings, device telemetry
- **Financial Data**: Stock prices, trading volumes, market indicators
- **Scientific Data**: Experimental measurements, environmental monitoring

## Getting Help

- Read the [Getting Started Guide](getting-started.md)
- Check the [Troubleshooting Guide](troubleshooting.md)
- Review the [API Reference](api-reference.md)
- Look at the [Configuration Options](configuration.md)