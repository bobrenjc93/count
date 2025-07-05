use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::time::Duration;

pub type NodeId = u32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeStatus {
    Healthy,
    Degraded,
    Unreachable,
    Joining,
    Leaving,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub address: SocketAddr,
    pub status: NodeStatus,
    pub last_seen: u64, // Unix timestamp in milliseconds
    pub version: String,
    pub data_size: u64, // Approximate data size in bytes
}

impl Node {
    pub fn new(id: NodeId, address: SocketAddr) -> Self {
        Self {
            id,
            address,
            status: NodeStatus::Joining,
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            version: env!("CARGO_PKG_VERSION").to_string(),
            data_size: 0,
        }
    }

    pub fn update_last_seen(&mut self) {
        self.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self.status, NodeStatus::Healthy)
    }

    pub fn is_available(&self) -> bool {
        matches!(self.status, NodeStatus::Healthy | NodeStatus::Degraded)
    }

    pub fn time_since_last_seen(&self) -> Duration {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Duration::from_millis(now.saturating_sub(self.last_seen))
    }

    pub fn should_be_considered_dead(&self, timeout: Duration) -> bool {
        self.time_since_last_seen() > timeout
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_usage: f64,
    pub network_io: NetworkIO,
    pub active_connections: u32,
    pub queries_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkIO {
    pub bytes_in: u64,
    pub bytes_out: u64,
}

impl Default for NodeMetrics {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_usage: 0.0,
            network_io: NetworkIO {
                bytes_in: 0,
                bytes_out: 0,
            },
            active_connections: 0,
            queries_per_second: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_node_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let node = Node::new(1, addr);
        
        assert_eq!(node.id, 1);
        assert_eq!(node.address, addr);
        assert_eq!(node.status, NodeStatus::Joining);
        assert!(node.time_since_last_seen() < Duration::from_millis(100));
    }

    #[test]
    fn test_node_status_checks() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let mut node = Node::new(1, addr);
        
        node.status = NodeStatus::Healthy;
        assert!(node.is_healthy());
        assert!(node.is_available());
        
        node.status = NodeStatus::Degraded;
        assert!(!node.is_healthy());
        assert!(node.is_available());
        
        node.status = NodeStatus::Unreachable;
        assert!(!node.is_healthy());
        assert!(!node.is_available());
    }

    #[test]
    fn test_node_timeout() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let mut node = Node::new(1, addr);
        
        // Simulate old timestamp
        node.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64 - 10000; // 10 seconds ago
        
        assert!(node.should_be_considered_dead(Duration::from_secs(5)));
        assert!(!node.should_be_considered_dead(Duration::from_secs(15)));
    }
}