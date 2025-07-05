use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type NodeId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardKey {
    pub series: String,
    pub hash: u64,
}

impl ShardKey {
    pub fn new(series: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(series.as_bytes());
        let result = hasher.finalize();
        
        // Convert first 8 bytes to u64
        let hash = u64::from_be_bytes([
            result[0], result[1], result[2], result[3],
            result[4], result[5], result[6], result[7],
        ]);
        
        Self {
            series: series.to_string(),
            hash,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsistentHashRing {
    ring: Arc<RwLock<BTreeMap<u64, NodeId>>>,
    virtual_nodes: usize,
}

impl ConsistentHashRing {
    pub fn new(virtual_nodes: usize) -> Self {
        Self {
            ring: Arc::new(RwLock::new(BTreeMap::new())),
            virtual_nodes,
        }
    }

    pub async fn add_node(&self, node_id: NodeId) {
        let mut ring = self.ring.write().await;
        
        for i in 0..self.virtual_nodes {
            let virtual_key = format!("{}:{}", node_id, i);
            let mut hasher = Sha256::new();
            hasher.update(virtual_key.as_bytes());
            let result = hasher.finalize();
            
            let hash = u64::from_be_bytes([
                result[0], result[1], result[2], result[3],
                result[4], result[5], result[6], result[7],
            ]);
            
            ring.insert(hash, node_id);
        }
    }

    pub async fn remove_node(&self, node_id: NodeId) {
        let mut ring = self.ring.write().await;
        
        // Remove all virtual nodes for this physical node
        let keys_to_remove: Vec<u64> = ring
            .iter()
            .filter(|(_, &v)| v == node_id)
            .map(|(&k, _)| k)
            .collect();
            
        for key in keys_to_remove {
            ring.remove(&key);
        }
    }

    pub async fn get_node(&self, shard_key: &ShardKey) -> Option<NodeId> {
        let ring = self.ring.read().await;
        
        if ring.is_empty() {
            return None;
        }
        
        // Find the first node with hash >= shard_key.hash
        if let Some((&_hash, &node_id)) = ring.range(shard_key.hash..).next() {
            Some(node_id)
        } else {
            // Wrap around to the beginning of the ring
            ring.iter().next().map(|(_, &node_id)| node_id)
        }
    }

    pub async fn get_nodes_for_replication(
        &self, 
        shard_key: &ShardKey, 
        replication_factor: usize
    ) -> Vec<NodeId> {
        let ring = self.ring.read().await;
        let mut nodes = Vec::new();
        let mut seen_physical_nodes = std::collections::HashSet::new();
        
        if ring.is_empty() {
            return nodes;
        }
        
        // Start from the primary node position
        let start_iter = ring.range(shard_key.hash..);
        let wrap_iter = ring.range(..);
        
        for (_, &node_id) in start_iter.chain(wrap_iter) {
            if seen_physical_nodes.insert(node_id) {
                nodes.push(node_id);
                if nodes.len() >= replication_factor {
                    break;
                }
            }
        }
        
        nodes
    }

    pub async fn get_all_nodes(&self) -> Vec<NodeId> {
        let ring = self.ring.read().await;
        let mut unique_nodes: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
        
        for &node_id in ring.values() {
            unique_nodes.insert(node_id);
        }
        
        unique_nodes.into_iter().collect()
    }

    pub async fn is_empty(&self) -> bool {
        let ring = self.ring.read().await;
        ring.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consistent_hash_ring() {
        let ring = ConsistentHashRing::new(150);
        
        // Add some nodes
        ring.add_node(1).await;
        ring.add_node(2).await;
        ring.add_node(3).await;
        
        let shard_key = ShardKey::new("cpu.usage.total");
        let node = ring.get_node(&shard_key).await;
        assert!(node.is_some());
        
        // Test replication
        let replica_nodes = ring.get_nodes_for_replication(&shard_key, 2).await;
        assert_eq!(replica_nodes.len(), 2);
        assert!(replica_nodes.contains(&node.unwrap()));
    }

    #[tokio::test]
    async fn test_node_removal() {
        let ring = ConsistentHashRing::new(150);
        
        ring.add_node(1).await;
        ring.add_node(2).await;
        ring.add_node(3).await;
        
        let all_nodes_before = ring.get_all_nodes().await;
        assert_eq!(all_nodes_before.len(), 3);
        
        ring.remove_node(2).await;
        
        let all_nodes_after = ring.get_all_nodes().await;
        assert_eq!(all_nodes_after.len(), 2);
        assert!(!all_nodes_after.contains(&2));
    }

    #[tokio::test]
    async fn test_shard_key_consistency() {
        let key1 = ShardKey::new("cpu.usage.total");
        let key2 = ShardKey::new("cpu.usage.total");
        
        assert_eq!(key1.hash, key2.hash);
        assert_eq!(key1.series, key2.series);
    }
}