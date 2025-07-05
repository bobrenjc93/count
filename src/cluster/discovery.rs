use crate::cluster::{Node, NodeId, NodeStatus};
use crate::error::CountResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    pub sender_id: NodeId,
    pub message_type: GossipMessageType,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessageType {
    Heartbeat { node: Node },
    NodeJoin { node: Node },
    NodeLeave { node_id: NodeId },
    NodeFailed { node_id: NodeId },
    ClusterState { nodes: Vec<Node> },
}

#[derive(Debug)]
pub struct DiscoveryConfig {
    pub heartbeat_interval: Duration,
    pub failure_timeout: Duration,
    pub gossip_fanout: usize,
    pub max_gossip_rounds: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(5),
            failure_timeout: Duration::from_secs(30),
            gossip_fanout: 3,
            max_gossip_rounds: 3,
        }
    }
}

pub struct DiscoveryService {
    local_node: Node,
    known_nodes: Arc<RwLock<HashMap<NodeId, Node>>>,
    config: DiscoveryConfig,
    client: reqwest::Client,
}

impl DiscoveryService {
    pub fn new(local_node: Node, config: DiscoveryConfig) -> Self {
        let mut known_nodes = HashMap::new();
        known_nodes.insert(local_node.id, local_node.clone());

        Self {
            local_node,
            known_nodes: Arc::new(RwLock::new(known_nodes)),
            config,
            client: reqwest::Client::new(),
        }
    }

    pub async fn start(&self, seed_nodes: Vec<SocketAddr>) -> CountResult<()> {
        info!("Starting discovery service for node {}", self.local_node.id);

        // Join existing cluster if seed nodes are provided
        if !seed_nodes.is_empty() {
            self.join_cluster(seed_nodes).await?;
        }

        // Start heartbeat task
        let heartbeat_service = self.clone_for_task();
        tokio::spawn(async move {
            heartbeat_service.heartbeat_loop().await;
        });

        // Start failure detection task
        let failure_detection_service = self.clone_for_task();
        tokio::spawn(async move {
            failure_detection_service.failure_detection_loop().await;
        });

        Ok(())
    }

    async fn join_cluster(&self, seed_nodes: Vec<SocketAddr>) -> CountResult<()> {
        info!("Attempting to join cluster via seed nodes: {:?}", seed_nodes);

        for seed_addr in seed_nodes {
            match self.request_cluster_join(seed_addr).await {
                Ok(_) => {
                    info!("Successfully joined cluster via {}", seed_addr);
                    return Ok(());
                }
                Err(e) => {
                    warn!("Failed to join via {}: {}", seed_addr, e);
                    continue;
                }
            }
        }

        warn!("Failed to join cluster via any seed node, starting as single node");
        Ok(())
    }

    async fn request_cluster_join(&self, seed_addr: SocketAddr) -> CountResult<()> {
        let join_message = GossipMessage {
            sender_id: self.local_node.id,
            message_type: GossipMessageType::NodeJoin {
                node: self.local_node.clone(),
            },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        let url = format!("http://{}/cluster/join", seed_addr);
        let response = self
            .client
            .post(&url)
            .json(&join_message)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| crate::error::CountError::NetworkError(e.to_string()))?;

        if response.status().is_success() {
            // Parse cluster state response
            let cluster_state: GossipMessage = response
                .json()
                .await
                .map_err(|e| crate::error::CountError::NetworkError(e.to_string()))?;

            if let GossipMessageType::ClusterState { nodes } = cluster_state.message_type {
                self.update_cluster_state(nodes).await;
            }
        }

        Ok(())
    }

    async fn heartbeat_loop(&self) {
        let mut interval = interval(self.config.heartbeat_interval);
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.send_heartbeats().await {
                error!("Error sending heartbeats: {}", e);
            }
        }
    }

    async fn send_heartbeats(&self) -> CountResult<()> {
        let nodes = self.get_alive_nodes().await;
        let mut local_node = self.local_node.clone();
        local_node.update_last_seen();

        let heartbeat_message = GossipMessage {
            sender_id: self.local_node.id,
            message_type: GossipMessageType::Heartbeat { node: local_node },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        // Send to a random subset of nodes (gossip fanout)
        let target_count = std::cmp::min(self.config.gossip_fanout, nodes.len());
        let mut selected_nodes: Vec<_> = nodes.into_iter().collect();
        selected_nodes.sort_by_key(|_| rand::random::<u32>());
        selected_nodes.truncate(target_count);

        for node in selected_nodes {
            if node.id != self.local_node.id {
                let _ = self.send_gossip_message(&node, &heartbeat_message).await;
            }
        }

        Ok(())
    }

    async fn failure_detection_loop(&self) {
        let mut interval = interval(self.config.heartbeat_interval);
        
        loop {
            interval.tick().await;
            
            self.detect_failed_nodes().await;
        }
    }

    async fn detect_failed_nodes(&self) {
        let mut nodes_to_mark_failed = Vec::new();
        
        {
            let known_nodes = self.known_nodes.read().await;
            for node in known_nodes.values() {
                if node.id != self.local_node.id 
                    && node.should_be_considered_dead(self.config.failure_timeout) 
                    && node.status != NodeStatus::Unreachable {
                    nodes_to_mark_failed.push(node.id);
                }
            }
        }

        for node_id in nodes_to_mark_failed {
            warn!("Marking node {} as failed", node_id);
            self.mark_node_failed(node_id).await;
        }
    }

    async fn mark_node_failed(&self, node_id: NodeId) {
        {
            let mut known_nodes = self.known_nodes.write().await;
            if let Some(node) = known_nodes.get_mut(&node_id) {
                node.status = NodeStatus::Unreachable;
            }
        }

        // Gossip the failure to other nodes
        let failure_message = GossipMessage {
            sender_id: self.local_node.id,
            message_type: GossipMessageType::NodeFailed { node_id },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        let alive_nodes = self.get_alive_nodes().await;
        for node in alive_nodes {
            if node.id != self.local_node.id {
                let _ = self.send_gossip_message(&node, &failure_message).await;
            }
        }
    }

    async fn send_gossip_message(&self, target: &Node, message: &GossipMessage) -> CountResult<()> {
        let url = format!("http://{}/cluster/gossip", target.address);
        
        let response = self
            .client
            .post(&url)
            .json(message)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| crate::error::CountError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(crate::error::CountError::NetworkError(
                format!("Gossip failed with status: {}", response.status())
            ));
        }

        Ok(())
    }

    pub async fn handle_gossip_message(&self, message: GossipMessage) -> CountResult<()> {
        debug!("Received gossip message: {:?}", message);

        match message.message_type {
            GossipMessageType::Heartbeat { node } => {
                self.update_node_info(node).await;
            }
            GossipMessageType::NodeJoin { node } => {
                info!("Node {} joining cluster", node.id);
                self.add_node(node).await;
            }
            GossipMessageType::NodeLeave { node_id } => {
                info!("Node {} leaving cluster", node_id);
                self.remove_node(node_id).await;
            }
            GossipMessageType::NodeFailed { node_id } => {
                warn!("Node {} reported as failed", node_id);
                self.mark_node_failed(node_id).await;
            }
            GossipMessageType::ClusterState { nodes } => {
                self.update_cluster_state(nodes).await;
            }
        }

        Ok(())
    }

    async fn update_node_info(&self, mut node: Node) {
        node.update_last_seen();
        if node.status == NodeStatus::Unreachable {
            node.status = NodeStatus::Healthy;
            info!("Node {} is back online", node.id);
        }

        let mut known_nodes = self.known_nodes.write().await;
        known_nodes.insert(node.id, node);
    }

    async fn add_node(&self, node: Node) {
        let mut known_nodes = self.known_nodes.write().await;
        known_nodes.insert(node.id, node);
    }

    async fn remove_node(&self, node_id: NodeId) {
        let mut known_nodes = self.known_nodes.write().await;
        known_nodes.remove(&node_id);
    }

    async fn update_cluster_state(&self, nodes: Vec<Node>) {
        let mut known_nodes = self.known_nodes.write().await;
        for node in nodes {
            known_nodes.insert(node.id, node);
        }
    }

    pub async fn get_alive_nodes(&self) -> Vec<Node> {
        let known_nodes = self.known_nodes.read().await;
        known_nodes
            .values()
            .filter(|node| node.is_available())
            .cloned()
            .collect()
    }

    pub async fn get_all_nodes(&self) -> Vec<Node> {
        let known_nodes = self.known_nodes.read().await;
        known_nodes.values().cloned().collect()
    }

    pub async fn get_cluster_state(&self) -> GossipMessage {
        let nodes = self.get_all_nodes().await;
        GossipMessage {
            sender_id: self.local_node.id,
            message_type: GossipMessageType::ClusterState { nodes },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }

    fn clone_for_task(&self) -> Self {
        Self {
            local_node: self.local_node.clone(),
            known_nodes: Arc::clone(&self.known_nodes),
            config: DiscoveryConfig {
                heartbeat_interval: self.config.heartbeat_interval,
                failure_timeout: self.config.failure_timeout,
                gossip_fanout: self.config.gossip_fanout,
                max_gossip_rounds: self.config.max_gossip_rounds,
            },
            client: self.client.clone(),
        }
    }
}