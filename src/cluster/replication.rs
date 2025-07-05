use crate::cluster::{ConsistentHashRing, Node, NodeId};
use crate::error::CountResult;
use crate::{DataPoint, SeriesKey};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationRequest {
    pub series: String,
    pub points: Vec<DataPoint>,
    pub replication_id: String,
    pub source_node: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationResponse {
    pub success: bool,
    pub error: Option<String>,
    pub node_id: NodeId,
}

pub struct ReplicationService {
    node_id: NodeId,
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
    replication_factor: usize,
    client: reqwest::Client,
}

impl ReplicationService {
    pub fn new(
        node_id: NodeId,
        hash_ring: Arc<RwLock<ConsistentHashRing>>,
        replication_factor: usize,
    ) -> Self {
        Self {
            node_id,
            hash_ring,
            replication_factor,
            client: reqwest::Client::new(),
        }
    }

    pub async fn replicate_write(
        &self,
        series: SeriesKey,
        point: DataPoint,
        nodes: &[Node],
    ) -> CountResult<Vec<ReplicationResponse>> {
        let replication_id = uuid::Uuid::new_v4().to_string();
        let request = ReplicationRequest {
            series: series.0.clone(),
            points: vec![point],
            replication_id,
            source_node: self.node_id,
        };

        let mut responses = Vec::new();
        let mut tasks = Vec::new();

        // Send replication requests to all replica nodes
        for node in nodes {
            if node.id != self.node_id {
                let client = self.client.clone();
                let req = request.clone();
                let node_addr = node.address.to_string();
                let node_id = node.id;

                let task = tokio::spawn(async move {
                    Self::send_replication_request(client, node_addr, req, node_id).await
                });
                tasks.push(task);
            }
        }

        // Wait for all replication requests to complete
        for task in tasks {
            match task.await {
                Ok(response) => responses.push(response),
                Err(e) => {
                    error!("Replication task failed: {}", e);
                    responses.push(ReplicationResponse {
                        success: false,
                        error: Some(e.to_string()),
                        node_id: 0, // Unknown node
                    });
                }
            }
        }

        Ok(responses)
    }

    async fn send_replication_request(
        client: reqwest::Client,
        node_addr: String,
        request: ReplicationRequest,
        node_id: NodeId,
    ) -> ReplicationResponse {
        let url = format!("http://{}/replicate", node_addr);
        
        match client
            .post(&url)
            .json(&request)
            .timeout(tokio::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<ReplicationResponse>().await {
                        Ok(resp) => resp,
                        Err(e) => ReplicationResponse {
                            success: false,
                            error: Some(format!("Failed to parse response: {}", e)),
                            node_id,
                        },
                    }
                } else {
                    ReplicationResponse {
                        success: false,
                        error: Some(format!("HTTP error: {}", response.status())),
                        node_id,
                    }
                }
            }
            Err(e) => ReplicationResponse {
                success: false,
                error: Some(format!("Network error: {}", e)),
                node_id,
            },
        }
    }

    pub async fn handle_replication_request(
        &self,
        request: ReplicationRequest,
        storage: &mut crate::storage::StorageEngine,
    ) -> ReplicationResponse {
        debug!(
            "Handling replication request from node {} for series {}",
            request.source_node, request.series
        );

        let series = SeriesKey::from(request.series.clone());
        
        for point in request.points {
            match storage.insert(series.clone(), point).await {
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to replicate data point: {}", e);
                    return ReplicationResponse {
                        success: false,
                        error: Some(e.to_string()),
                        node_id: self.node_id,
                    };
                }
            }
        }

        ReplicationResponse {
            success: true,
            error: None,
            node_id: self.node_id,
        }
    }

    pub async fn get_replica_nodes(&self, series: &SeriesKey) -> Vec<NodeId> {
        let hash_ring = self.hash_ring.read().await;
        let shard_key = crate::cluster::sharding::ShardKey::new(&series.0);
        hash_ring.get_nodes_for_replication(&shard_key, self.replication_factor).await
    }

    pub async fn is_write_successful(&self, responses: &[ReplicationResponse]) -> bool {
        let successful_replicas = responses.iter().filter(|r| r.success).count();
        let required_replicas = (self.replication_factor + 1) / 2; // Majority

        // Include local write as successful by default
        successful_replicas + 1 >= required_replicas
    }

    pub async fn repair_inconsistent_data(
        &self,
        series: SeriesKey,
        start_time: u64,
        end_time: u64,
        nodes: &[Node],
    ) -> CountResult<()> {
        warn!(
            "Starting data repair for series {} from {} to {}",
            series.0, start_time, end_time
        );

        // This is a simplified repair process
        // In a production system, you'd want more sophisticated conflict resolution
        
        let mut all_data: Vec<(NodeId, Vec<DataPoint>)> = Vec::new();
        
        // Collect data from all replica nodes
        for node in nodes {
            if let Ok(data) = self.fetch_data_from_node(node, &series, start_time, end_time).await {
                all_data.push((node.id, data));
            }
        }

        // Find the most complete dataset (simple heuristic)
        if let Some((_, canonical_data)) = all_data
            .iter()
            .max_by_key(|(_, data)| data.len())
        {
            // Repair inconsistent nodes
            for (node_id, node_data) in &all_data {
                if node_data.len() < canonical_data.len() {
                    warn!("Repairing node {} with {} missing data points", 
                          node_id, canonical_data.len() - node_data.len());
                    
                    // Find missing points and send them to the node
                    let missing_points: Vec<_> = canonical_data
                        .iter()
                        .filter(|point| !node_data.contains(point))
                        .cloned()
                        .collect();

                    if !missing_points.is_empty() {
                        let repair_request = ReplicationRequest {
                            series: series.0.clone(),
                            points: missing_points,
                            replication_id: format!("repair-{}", uuid::Uuid::new_v4()),
                            source_node: self.node_id,
                        };

                        if let Some(node) = nodes.iter().find(|n| n.id == *node_id) {
                            let _ = Self::send_replication_request(
                                self.client.clone(),
                                node.address.to_string(),
                                repair_request,
                                *node_id,
                            ).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn fetch_data_from_node(
        &self,
        node: &Node,
        series: &SeriesKey,
        start_time: u64,
        end_time: u64,
    ) -> CountResult<Vec<DataPoint>> {
        let url = format!(
            "http://{}/query/range?series={}&start={}&end={}",
            node.address, series.0, start_time, end_time
        );

        let response = self
            .client
            .get(&url)
            .timeout(tokio::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| crate::error::CountError::NetworkError(e.to_string()))?;

        if response.status().is_success() {
            let data: Vec<DataPoint> = response
                .json()
                .await
                .map_err(|e| crate::error::CountError::NetworkError(e.to_string()))?;
            Ok(data)
        } else {
            Err(crate::error::CountError::NetworkError(
                format!("Failed to fetch data from node {}: {}", node.id, response.status())
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::sharding::ConsistentHashRing;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_replication_service() {
        let hash_ring = Arc::new(RwLock::new(ConsistentHashRing::new(150)));
        let replication_service = ReplicationService::new(1, hash_ring, 2);

        let nodes = vec![
            Node::new(2, SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8082)),
            Node::new(3, SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8083)),
        ];

        let series = SeriesKey::from("test.series");
        let point = DataPoint::new(1000, 42.0);

        // Note: This test would require running HTTP servers to fully test
        // For now, we just test that the method doesn't panic
        let _responses = replication_service.replicate_write(series, point, &nodes).await;
    }

    #[test]
    fn test_replication_response() {
        let response = ReplicationResponse {
            success: true,
            error: None,
            node_id: 1,
        };

        assert!(response.success);
        assert!(response.error.is_none());
        assert_eq!(response.node_id, 1);
    }
}