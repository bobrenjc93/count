use crate::cluster::{ConsistentHashRing, DiscoveryService, Node, NodeId, ReplicationService};
use crate::distributed::DistributedQueryRouter;
use crate::error::CountResult;
use crate::storage::StorageEngine;
use crate::{CountConfig, DataPoint, SeriesKey};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub struct ClusterConfig {
    pub node_id: NodeId,
    pub bind_address: SocketAddr,
    pub seed_nodes: Vec<SocketAddr>,
    pub replication_factor: usize,
    pub virtual_nodes: usize,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            node_id: 1,
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            seed_nodes: Vec::new(),
            replication_factor: 2,
            virtual_nodes: 150,
        }
    }
}

pub struct ClusterCoordinator {
    local_node: Node,
    config: ClusterConfig,
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
    storage: Arc<RwLock<StorageEngine>>,
    discovery: Arc<DiscoveryService>,
    replication: Arc<ReplicationService>,
    query_router: Arc<DistributedQueryRouter>,
    nodes: Arc<RwLock<HashMap<NodeId, Node>>>,
}

impl ClusterCoordinator {
    pub async fn new(
        count_config: CountConfig,
        cluster_config: ClusterConfig,
    ) -> CountResult<Self> {
        info!("Initializing cluster coordinator for node {}", cluster_config.node_id);

        let local_node = Node::new(cluster_config.node_id, cluster_config.bind_address);
        let hash_ring = Arc::new(RwLock::new(ConsistentHashRing::new(cluster_config.virtual_nodes)));
        let storage = Arc::new(RwLock::new(StorageEngine::new(&count_config).await?));
        let nodes = Arc::new(RwLock::new(HashMap::new()));

        // Initialize discovery service
        let discovery_config = crate::cluster::discovery::DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(local_node.clone(), discovery_config));

        // Initialize replication service
        let replication = Arc::new(ReplicationService::new(
            cluster_config.node_id,
            Arc::clone(&hash_ring),
            cluster_config.replication_factor,
        ));

        // Initialize query router
        let query_router = Arc::new(DistributedQueryRouter::new(
            cluster_config.node_id,
            Arc::clone(&hash_ring),
            Arc::clone(&nodes),
        ));

        // Add local node to the hash ring
        {
            let mut ring = hash_ring.write().await;
            ring.add_node(cluster_config.node_id).await;
        }

        // Add local node to nodes map
        {
            let mut nodes_map = nodes.write().await;
            nodes_map.insert(local_node.id, local_node.clone());
        }

        Ok(Self {
            local_node,
            config: cluster_config,
            hash_ring,
            storage,
            discovery,
            replication,
            query_router,
            nodes,
        })
    }

    pub async fn start(&self) -> CountResult<()> {
        info!("Starting cluster coordinator");

        // Start discovery service
        self.discovery.start(self.config.seed_nodes.clone()).await?;

        info!("Cluster coordinator started successfully");
        Ok(())
    }

    pub async fn insert(&self, series: SeriesKey, point: DataPoint) -> CountResult<()> {
        // Determine which nodes should store this data
        let shard_key = crate::cluster::sharding::ShardKey::new(&series.0);
        let replica_nodes = {
            let hash_ring = self.hash_ring.read().await;
            hash_ring.get_nodes_for_replication(&shard_key, self.config.replication_factor).await
        };

        // Check if local node is a replica
        let is_local_replica = replica_nodes.contains(&self.config.node_id);

        // Store locally if we're a replica
        if is_local_replica {
            let mut storage = self.storage.write().await;
            storage.insert(series.clone(), point.clone()).await?;
        }

        // Replicate to other nodes
        if replica_nodes.len() > 1 {
            let nodes_to_replicate: Vec<Node> = {
                let nodes = self.nodes.read().await;
                replica_nodes
                    .iter()
                    .filter_map(|&node_id| nodes.get(&node_id).cloned())
                    .collect()
            };

            let replication_responses = self
                .replication
                .replicate_write(series, point, &nodes_to_replicate)
                .await?;

            // Check if write was successful (majority of replicas)
            if !self.replication.is_write_successful(&replication_responses).await {
                warn!("Write may not have achieved required consistency level");
                // In a production system, you might want to retry or return an error
            }
        }

        Ok(())
    }

    pub async fn query_range(
        &self,
        series: SeriesKey,
        start_time: u64,
        end_time: u64,
    ) -> CountResult<Vec<DataPoint>> {
        let storage = self.storage.read().await;
        let result = self
            .query_router
            .execute_distributed_query(series, start_time, end_time, None, &*storage)
            .await?;

        if !result.partial_failures.is_empty() {
            warn!("Query had partial failures: {:?}", result.partial_failures);
        }

        Ok(result.data)
    }

    pub async fn query_aggregated(
        &self,
        series: SeriesKey,
        start_time: u64,
        end_time: u64,
        aggregation: crate::query::AggregationType,
    ) -> CountResult<f64> {
        let storage = self.storage.read().await;
        let result = self
            .query_router
            .execute_distributed_query(series, start_time, end_time, Some(aggregation), &*storage)
            .await?;

        if !result.partial_failures.is_empty() {
            warn!("Query had partial failures: {:?}", result.partial_failures);
        }

        Ok(result.aggregated_value.unwrap_or(0.0))
    }

    pub async fn add_node(&self, node: Node) -> CountResult<()> {
        info!("Adding node {} to cluster", node.id);

        // Add to hash ring
        {
            let mut hash_ring = self.hash_ring.write().await;
            hash_ring.add_node(node.id).await;
        }

        // Add to nodes map
        {
            let mut nodes = self.nodes.write().await;
            nodes.insert(node.id, node);
        }

        // TODO: Trigger data rebalancing
        self.trigger_rebalancing().await?;

        Ok(())
    }

    pub async fn remove_node(&self, node_id: NodeId) -> CountResult<()> {
        info!("Removing node {} from cluster", node_id);

        // Remove from hash ring
        {
            let mut hash_ring = self.hash_ring.write().await;
            hash_ring.remove_node(node_id).await;
        }

        // Remove from nodes map
        {
            let mut nodes = self.nodes.write().await;
            nodes.remove(&node_id);
        }

        // TODO: Trigger data rebalancing and repair
        self.trigger_rebalancing().await?;

        Ok(())
    }

    async fn trigger_rebalancing(&self) -> CountResult<()> {
        info!("Triggering cluster rebalancing");
        
        // In a production system, this would involve:
        // 1. Determining which data needs to be moved
        // 2. Streaming data to new nodes
        // 3. Verifying data integrity
        // 4. Cleaning up old data
        
        // For now, we'll just log that rebalancing should occur
        warn!("Data rebalancing not implemented - manual intervention may be required");
        
        Ok(())
    }

    pub async fn get_cluster_status(&self) -> ClusterStatus {
        let nodes = self.discovery.get_all_nodes().await;
        let alive_nodes = self.discovery.get_alive_nodes().await;
        
        let ring_empty = {
            let hash_ring = self.hash_ring.read().await;
            hash_ring.is_empty().await
        };

        ClusterStatus {
            local_node_id: self.config.node_id,
            total_nodes: nodes.len(),
            healthy_nodes: alive_nodes.len(),
            ring_initialized: !ring_empty,
            replication_factor: self.config.replication_factor,
        }
    }

    pub async fn handle_gossip_message(&self, message: crate::cluster::discovery::GossipMessage) -> CountResult<()> {
        self.discovery.handle_gossip_message(message).await
    }

    pub async fn handle_replication_request(
        &self,
        request: crate::cluster::replication::ReplicationRequest,
    ) -> crate::cluster::replication::ReplicationResponse {
        let mut storage = self.storage.write().await;
        self.replication.handle_replication_request(request, &mut *storage).await
    }

    pub async fn handle_query_request(
        &self,
        request: crate::distributed::query_router::QueryRequest,
    ) -> crate::distributed::query_router::QueryResponse {
        let storage = self.storage.read().await;
        self.query_router.handle_query_request(request, &*storage).await
    }

    pub async fn shutdown(&self) -> CountResult<()> {
        info!("Shutting down cluster coordinator");

        // Graceful shutdown - flush data and notify other nodes
        {
            let mut storage = self.storage.write().await;
            storage.shutdown().await?;
        }

        info!("Cluster coordinator shutdown complete");
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ClusterStatus {
    pub local_node_id: NodeId,
    pub total_nodes: usize,
    pub healthy_nodes: usize,
    pub ring_initialized: bool,
    pub replication_factor: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CountConfig;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_cluster_coordinator_creation() {
        let count_config = CountConfig::default();
        let cluster_config = ClusterConfig {
            node_id: 1,
            bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            seed_nodes: vec![],
            replication_factor: 2,
            virtual_nodes: 150,
        };

        let coordinator = ClusterCoordinator::new(count_config, cluster_config).await;
        assert!(coordinator.is_ok());
    }

    #[tokio::test]
    async fn test_cluster_status() {
        let count_config = CountConfig::default();
        let cluster_config = ClusterConfig::default();

        let coordinator = ClusterCoordinator::new(count_config, cluster_config).await.unwrap();
        let status = coordinator.get_cluster_status().await;

        assert_eq!(status.local_node_id, 1);
        assert_eq!(status.replication_factor, 2);
        assert!(status.ring_initialized);
    }
}