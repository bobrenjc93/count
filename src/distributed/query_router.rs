use crate::cluster::{ConsistentHashRing, Node, NodeId};
use crate::error::CountResult;
use crate::query::AggregationType;
use crate::{DataPoint, SeriesKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    pub series: String,
    pub start_time: u64,
    pub end_time: u64,
    pub aggregation: Option<AggregationType>,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    pub success: bool,
    pub data: Option<Vec<DataPoint>>,
    pub aggregated_value: Option<f64>,
    pub error: Option<String>,
    pub node_id: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedQueryResult {
    pub data: Vec<DataPoint>,
    pub aggregated_value: Option<f64>,
    pub nodes_queried: Vec<NodeId>,
    pub partial_failures: Vec<String>,
}

pub struct DistributedQueryRouter {
    local_node_id: NodeId,
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
    nodes: Arc<RwLock<HashMap<NodeId, Node>>>,
    client: reqwest::Client,
    read_timeout: tokio::time::Duration,
}

impl DistributedQueryRouter {
    pub fn new(
        local_node_id: NodeId,
        hash_ring: Arc<RwLock<ConsistentHashRing>>,
        nodes: Arc<RwLock<HashMap<NodeId, Node>>>,
    ) -> Self {
        Self {
            local_node_id,
            hash_ring,
            nodes,
            client: reqwest::Client::new(),
            read_timeout: tokio::time::Duration::from_secs(30),
        }
    }

    pub async fn execute_distributed_query(
        &self,
        series: SeriesKey,
        start_time: u64,
        end_time: u64,
        aggregation: Option<AggregationType>,
        local_storage: &crate::storage::StorageEngine,
    ) -> CountResult<DistributedQueryResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        info!(
            "Executing distributed query for series {} ({})",
            series.0, request_id
        );

        // Determine which nodes have data for this series
        let target_nodes = self.get_nodes_for_series(&series).await?;
        debug!("Target nodes for query: {:?}", target_nodes);

        let mut query_tasks = Vec::new();
        let mut responses = Vec::new();

        // Execute query on local node if we're a target
        let local_response = if target_nodes.iter().any(|&id| id == self.local_node_id) {
            Some(self.execute_local_query(&series, start_time, end_time, aggregation, local_storage).await)
        } else {
            None
        };

        // Execute queries on remote nodes
        for &node_id in &target_nodes {
            if node_id != self.local_node_id {
                if let Some(node) = self.get_node_info(node_id).await {
                    let client = self.client.clone();
                    let query_req = QueryRequest {
                        series: series.0.clone(),
                        start_time,
                        end_time,
                        aggregation,
                        request_id: request_id.clone(),
                    };
                    let node_addr = node.address.to_string();

                    let task = tokio::spawn(async move {
                        Self::execute_remote_query(client, node_addr, query_req, node_id).await
                    });
                    query_tasks.push(task);
                }
            }
        }

        // Collect local response
        if let Some(local_resp) = local_response {
            responses.push(local_resp);
        }

        // Wait for remote queries to complete
        for task in query_tasks {
            match task.await {
                Ok(response) => responses.push(response),
                Err(e) => {
                    error!("Query task failed: {}", e);
                    responses.push(QueryResponse {
                        success: false,
                        data: None,
                        aggregated_value: None,
                        error: Some(e.to_string()),
                        node_id: 0, // Unknown node
                    });
                }
            }
        }

        // Aggregate results from all nodes
        self.aggregate_query_responses(responses, aggregation).await
    }

    async fn execute_local_query(
        &self,
        series: &SeriesKey,
        start_time: u64,
        end_time: u64,
        aggregation: Option<AggregationType>,
        storage: &crate::storage::StorageEngine,
    ) -> QueryResponse {
        debug!("Executing local query for series {}", series.0);

        match aggregation {
            Some(agg_type) => {
                match storage.query_aggregated(series.clone(), start_time, end_time, agg_type).await {
                    Ok(value) => QueryResponse {
                        success: true,
                        data: None,
                        aggregated_value: Some(value),
                        error: None,
                        node_id: self.local_node_id,
                    },
                    Err(e) => QueryResponse {
                        success: false,
                        data: None,
                        aggregated_value: None,
                        error: Some(e.to_string()),
                        node_id: self.local_node_id,
                    },
                }
            }
            None => {
                match storage.query_range(series.clone(), start_time, end_time).await {
                    Ok(data) => QueryResponse {
                        success: true,
                        data: Some(data),
                        aggregated_value: None,
                        error: None,
                        node_id: self.local_node_id,
                    },
                    Err(e) => QueryResponse {
                        success: false,
                        data: None,
                        aggregated_value: None,
                        error: Some(e.to_string()),
                        node_id: self.local_node_id,
                    },
                }
            }
        }
    }

    async fn execute_remote_query(
        client: reqwest::Client,
        node_addr: String,
        request: QueryRequest,
        node_id: NodeId,
    ) -> QueryResponse {
        let url = if request.aggregation.is_some() {
            format!("http://{}/query/aggregated", node_addr)
        } else {
            format!("http://{}/query/range", node_addr)
        };

        match client
            .post(&url)
            .json(&request)
            .timeout(tokio::time::Duration::from_secs(30))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<QueryResponse>().await {
                        Ok(resp) => resp,
                        Err(e) => QueryResponse {
                            success: false,
                            data: None,
                            aggregated_value: None,
                            error: Some(format!("Failed to parse response: {}", e)),
                            node_id,
                        },
                    }
                } else {
                    QueryResponse {
                        success: false,
                        data: None,
                        aggregated_value: None,
                        error: Some(format!("HTTP error: {}", response.status())),
                        node_id,
                    }
                }
            }
            Err(e) => QueryResponse {
                success: false,
                data: None,
                aggregated_value: None,
                error: Some(format!("Network error: {}", e)),
                node_id,
            },
        }
    }

    async fn aggregate_query_responses(
        &self,
        responses: Vec<QueryResponse>,
        aggregation: Option<AggregationType>,
    ) -> CountResult<DistributedQueryResult> {
        let mut all_data = Vec::new();
        let mut nodes_queried = Vec::new();
        let mut partial_failures = Vec::new();
        let mut aggregated_values = Vec::new();

        // Process responses
        for response in responses {
            nodes_queried.push(response.node_id);
            
            if response.success {
                if let Some(data) = response.data {
                    all_data.extend(data);
                }
                if let Some(value) = response.aggregated_value {
                    aggregated_values.push(value);
                }
            } else {
                if let Some(error) = response.error {
                    partial_failures.push(format!("Node {}: {}", response.node_id, error));
                }
            }
        }

        // Sort and deduplicate data points
        all_data.sort_by_key(|p| p.timestamp);
        all_data.dedup_by_key(|p| p.timestamp);

        // Calculate final aggregated value if needed
        let final_aggregated_value = match aggregation {
            Some(agg_type) if !aggregated_values.is_empty() => {
                Some(self.combine_aggregated_values(aggregated_values, agg_type, &all_data))
            }
            Some(agg_type) if !all_data.is_empty() => {
                // Fall back to aggregating the raw data
                Some(self.aggregate_data_points(&all_data, agg_type))
            }
            _ => None,
        };

        Ok(DistributedQueryResult {
            data: all_data,
            aggregated_value: final_aggregated_value,
            nodes_queried,
            partial_failures,
        })
    }

    fn combine_aggregated_values(
        &self,
        values: Vec<f64>,
        aggregation: AggregationType,
        raw_data: &[DataPoint],
    ) -> f64 {
        match aggregation {
            AggregationType::Sum => values.iter().sum(),
            AggregationType::Count => values.iter().sum(),
            AggregationType::Mean => {
                if !raw_data.is_empty() {
                    // Recalculate mean from raw data for accuracy
                    self.aggregate_data_points(raw_data, aggregation)
                } else {
                    // Approximate from available means (not accurate for distributed means)
                    values.iter().sum::<f64>() / values.len() as f64
                }
            }
            AggregationType::Min => values.iter().fold(f64::INFINITY, |a, &b| f64::min(a, b)),
            AggregationType::Max => values.iter().fold(f64::NEG_INFINITY, |a, &b| f64::max(a, b)),
        }
    }

    fn aggregate_data_points(&self, data: &[DataPoint], aggregation: AggregationType) -> f64 {
        if data.is_empty() {
            return 0.0;
        }

        match aggregation {
            AggregationType::Sum => data.iter().map(|p| p.value).sum(),
            AggregationType::Count => data.len() as f64,
            AggregationType::Mean => {
                let sum: f64 = data.iter().map(|p| p.value).sum();
                sum / data.len() as f64
            }
            AggregationType::Min => data.iter().map(|p| p.value).fold(f64::INFINITY, f64::min),
            AggregationType::Max => data.iter().map(|p| p.value).fold(f64::NEG_INFINITY, f64::max),
        }
    }

    async fn get_nodes_for_series(&self, series: &SeriesKey) -> CountResult<Vec<NodeId>> {
        let hash_ring = self.hash_ring.read().await;
        let shard_key = crate::cluster::sharding::ShardKey::new(&series.0);
        
        // For read queries, we need to query all replica nodes to ensure consistency
        let replica_nodes = hash_ring.get_nodes_for_replication(&shard_key, 3).await; // Max 3 replicas
        
        if replica_nodes.is_empty() {
            warn!("No nodes found for series {}", series.0);
            return Err(crate::error::CountError::NetworkError(
                "No available nodes for query".to_string()
            ));
        }

        Ok(replica_nodes)
    }

    async fn get_node_info(&self, node_id: NodeId) -> Option<Node> {
        let nodes = self.nodes.read().await;
        nodes.get(&node_id).cloned()
    }

    pub async fn handle_query_request(
        &self,
        request: QueryRequest,
        storage: &crate::storage::StorageEngine,
    ) -> QueryResponse {
        debug!("Handling query request: {:?}", request);

        let series = SeriesKey::from(request.series);
        
        match request.aggregation {
            Some(agg_type) => {
                match storage.query_aggregated(series, request.start_time, request.end_time, agg_type).await {
                    Ok(value) => QueryResponse {
                        success: true,
                        data: None,
                        aggregated_value: Some(value),
                        error: None,
                        node_id: self.local_node_id,
                    },
                    Err(e) => QueryResponse {
                        success: false,
                        data: None,
                        aggregated_value: None,
                        error: Some(e.to_string()),
                        node_id: self.local_node_id,
                    },
                }
            }
            None => {
                match storage.query_range(series, request.start_time, request.end_time).await {
                    Ok(data) => QueryResponse {
                        success: true,
                        data: Some(data),
                        aggregated_value: None,
                        error: None,
                        node_id: self.local_node_id,
                    },
                    Err(e) => QueryResponse {
                        success: false,
                        data: None,
                        aggregated_value: None,
                        error: Some(e.to_string()),
                        node_id: self.local_node_id,
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::sharding::ConsistentHashRing;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_query_router() {
        let hash_ring = Arc::new(RwLock::new(ConsistentHashRing::new(150)));
        let nodes = Arc::new(RwLock::new(HashMap::new()));
        
        let router = DistributedQueryRouter::new(1, hash_ring, nodes);
        
        let request = QueryRequest {
            series: "test.cpu".to_string(),
            start_time: 1000,
            end_time: 2000,
            aggregation: Some(AggregationType::Mean),
            request_id: "test-123".to_string(),
        };

        // Test that request parsing works
        assert_eq!(request.series, "test.cpu");
        assert_eq!(request.start_time, 1000);
        assert_eq!(request.end_time, 2000);
    }

    #[test]
    fn test_query_response() {
        let response = QueryResponse {
            success: true,
            data: Some(vec![DataPoint::new(1000, 42.0)]),
            aggregated_value: None,
            error: None,
            node_id: 1,
        };

        assert!(response.success);
        assert!(response.data.is_some());
        assert_eq!(response.data.unwrap().len(), 1);
    }
}