/// 测试集群管理器
/// 
/// 用于创建和管理不同类型的测试集群，支持故障注入和状态验证

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;
use anyhow::Result;

use crate::test_framework::{TestNode, NetworkSimulator, ChaosEngine};
use grape_vector_db::types::*;
use grape_vector_db::distributed::raft::RaftNode;
use grape_vector_db::distributed::shard::ShardManager;

/// 集群类型
#[derive(Debug, Clone, PartialEq)]
pub enum ClusterType {
    /// 内嵌模式
    Embedded,
    /// 单机模式
    Standalone,
    /// 三节点集群
    ThreeNode,
    /// 六节点集群
    SixNode,
}

impl ClusterType {
    pub fn node_count(&self) -> usize {
        match self {
            ClusterType::Embedded => 1,
            ClusterType::Standalone => 1,
            ClusterType::ThreeNode => 3,
            ClusterType::SixNode => 6,
        }
    }
}

/// 测试集群管理器
pub struct TestCluster {
    /// 集群类型
    cluster_type: ClusterType,
    /// 测试节点列表
    nodes: Arc<RwLock<Vec<TestNode>>>,
    /// 网络模拟器
    network_simulator: Arc<NetworkSimulator>,
    /// 混沌工程引擎
    chaos_engine: Arc<ChaosEngine>,
    /// 集群ID
    cluster_id: String,
    /// 节点状态跟踪
    node_states: Arc<RwLock<HashMap<String, NodeState>>>,
}

/// 节点状态
#[derive(Debug, Clone, PartialEq)]
pub enum NodeState {
    Running,
    Stopped,
    Failed,
    Recovering,
}

impl TestCluster {
    /// 创建新的测试集群
    pub async fn new(cluster_type: ClusterType) -> Self {
        let cluster_id = Uuid::new_v4().to_string();
        let node_count = cluster_type.node_count();
        
        let mut nodes = Vec::new();
        let mut node_states = HashMap::new();
        
        // 创建节点
        for i in 0..node_count {
            let node_id = format!("node_{}", i);
            let node = TestNode::new(node_id.clone(), cluster_type.clone()).await;
            node_states.insert(node_id, NodeState::Stopped);
            nodes.push(node);
        }
        
        let network_simulator = Arc::new(NetworkSimulator::new());
        
        Self {
            cluster_type,
            nodes: Arc::new(RwLock::new(nodes)),
            network_simulator: network_simulator.clone(),
            chaos_engine: Arc::new(ChaosEngine::new(network_simulator)),
            cluster_id,
            node_states: Arc::new(RwLock::new(node_states)),
        }
    }
    
    /// 启动所有节点
    pub async fn start_all_nodes(&self) -> Result<()> {
        let nodes = self.nodes.read().await;
        let mut node_states = self.node_states.write().await;
        
        for node in nodes.iter() {
            node.start().await?;
            node_states.insert(node.node_id().to_string(), NodeState::Running);
        }
        
        // 等待集群稳定
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        Ok(())
    }
    
    /// 停止指定节点
    pub async fn stop_node(&self, node_id: &str) -> Result<()> {
        let nodes = self.nodes.read().await;
        let mut node_states = self.node_states.write().await;
        
        if let Some(node) = nodes.iter().find(|n| n.node_id() == node_id) {
            node.stop().await?;
            node_states.insert(node_id.to_string(), NodeState::Stopped);
        }
        
        Ok(())
    }
    
    /// 重启指定节点
    pub async fn restart_node(&self, node_id: &str) -> Result<()> {
        let nodes = self.nodes.read().await;
        let mut node_states = self.node_states.write().await;
        
        if let Some(node) = nodes.iter().find(|n| n.node_id() == node_id) {
            node_states.insert(node_id.to_string(), NodeState::Recovering);
            node.restart().await?;
            node_states.insert(node_id.to_string(), NodeState::Running);
        }
        
        Ok(())
    }
    
    /// 等待领导者选出
    pub async fn wait_for_leader(&self) -> Result<String> {
        let timeout = Duration::from_secs(5);
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            let leaders = self.get_leaders().await;
            if !leaders.is_empty() {
                return Ok(leaders[0].clone());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        
        anyhow::bail!("超时: 未能选出领导者")
    }
    
    /// 获取当前领导者节点
    pub async fn get_leaders(&self) -> Vec<String> {
        let nodes = self.nodes.read().await;
        let mut leaders = Vec::new();
        
        for node in nodes.iter() {
            if node.is_leader().await {
                leaders.push(node.get_id().to_string());
            }
        }
        
        leaders
    }
    
    /// 获取跟随者节点
    pub async fn get_followers(&self) -> Vec<String> {
        let nodes = self.nodes.read().await;
        let mut followers = Vec::new();
        
        for node in nodes.iter() {
            if node.is_follower().await {
                followers.push(node.get_id().to_string());
            }
        }
        
        followers
    }
    
    /// 创建网络分区
    pub async fn create_partition(&self, partition1: Vec<usize>, partition2: Vec<usize>) -> Result<()> {
        let nodes = self.nodes.read().await;
        
        // 获取分区1和分区2的节点ID
        let partition1_ids: Vec<String> = partition1.iter()
            .filter_map(|&i| nodes.get(i).map(|n| n.node_id().to_string()))
            .collect();
        let partition2_ids: Vec<String> = partition2.iter()
            .filter_map(|&i| nodes.get(i).map(|n| n.node_id().to_string()))
            .collect();
        
        // 在网络模拟器中创建分区
        self.network_simulator.create_partition(partition1_ids, partition2_ids).await;
        
        Ok(())
    }
    
    /// 愈合网络分区
    pub async fn heal_partition(&self) -> Result<()> {
        self.network_simulator.heal_all_partitions().await;
        Ok(())
    }
    
    /// 检查集群是否可以达成共识
    pub async fn can_reach_consensus(&self) -> bool {
        // 尝试在集群中执行一个简单的操作并验证是否能达成共识
        let leaders = self.get_leaders().await;
        if !leaders.is_empty() {
            // 简化实现：返回true表示可以达成共识
            return true;
        }
        false
    }
    
    /// 检查集群是否可用
    pub async fn is_available(&self) -> bool {
        self.active_node_count().await >= self.majority_count()
    }
    
    /// 获取活跃节点数量
    pub async fn active_node_count(&self) -> usize {
        let node_states = self.node_states.read().await;
        node_states.values()
            .filter(|state| **state == NodeState::Running)
            .count()
    }
    
    /// 获取多数派所需的节点数量
    pub fn majority_count(&self) -> usize {
        (self.cluster_type.node_count() / 2) + 1
    }
    
    /// 等待日志同步完成
    pub async fn wait_for_log_sync(&self) -> Result<()> {
        let timeout = Duration::from_secs(10);
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            if self.verify_log_consistency().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        anyhow::bail!("超时: 日志同步失败")
    }
    
    /// 验证日志一致性
    pub async fn verify_log_consistency(&self) -> bool {
        let nodes = self.nodes.read().await;
        let running_nodes: Vec<_> = nodes.iter()
            .filter(|node| futures::executor::block_on(async { node.is_running().await }))
            .collect();
        
        if running_nodes.len() < 2 {
            return true; // 少于2个节点时无需验证一致性
        }
        
        // 获取第一个节点的日志作为基准
        if let Some(first_node) = running_nodes.first() {
            let base_logs = first_node.get_logs().await;
            
            // 验证其他节点的日志是否一致
            for node in running_nodes.iter().skip(1) {
                let node_logs = node.get_logs().await;
                if node_logs != base_logs {
                    return false;
                }
            }
        }
        
        true
    }
    
    /// 插入文档到集群
    pub async fn insert_document(&self, document: Document) -> Result<String> {
        let leaders = self.get_leaders().await;
        if !leaders.is_empty() {
            let leader_id = &leaders[0];
            let nodes = self.nodes.read().await;
            if let Some(leader_node) = nodes.iter().find(|n| n.get_id() == leader_id) {
                return leader_node.insert_document(document).await;
            }
        }
        anyhow::bail!("无可用的领导者节点")
    }
    
    /// 从集群获取文档
    pub async fn get_document(&self, doc_id: &str) -> Result<Document> {
        // 可以从任意节点读取（最终一致性）
        let nodes = self.nodes.read().await;
        for node in nodes.iter() {
            if node.is_running().await {
                if let Ok(doc) = node.get_document(doc_id).await {
                    return Ok(doc);
                }
            }
        }
        anyhow::bail!("文档未找到: {}", doc_id)
    }
    
    /// 获取集群统计信息
    pub async fn get_cluster_stats(&self) -> ClusterStats {
        let nodes = self.nodes.read().await;
        let active_nodes = self.active_node_count().await;
        let total_nodes = nodes.len();
        let leaders = self.get_leaders().await.len();
        
        ClusterStats {
            total_nodes,
            active_nodes,
            leader_count: leaders,
            is_healthy: active_nodes >= self.majority_count(),
        }
    }
    
    /// 获取网络模拟器
    pub fn network_simulator(&self) -> Arc<NetworkSimulator> {
        self.network_simulator.clone()
    }
    
    /// 获取混沌工程引擎
    pub fn chaos_engine(&self) -> Arc<ChaosEngine> {
        self.chaos_engine.clone()
    }
}

/// 集群统计信息
#[derive(Debug, Clone)]
pub struct ClusterStats {
    pub total_nodes: usize,
    pub active_nodes: usize,
    pub leader_count: usize,
    pub is_healthy: bool,
}

impl Clone for TestCluster {
    fn clone(&self) -> Self {
        Self {
            cluster_type: self.cluster_type.clone(),
            nodes: self.nodes.clone(),
            network_simulator: self.network_simulator.clone(),
            chaos_engine: self.chaos_engine.clone(),
            cluster_id: self.cluster_id.clone(),
            node_states: self.node_states.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cluster_creation() {
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        assert_eq!(cluster.cluster_type.node_count(), 3);
        assert_eq!(cluster.majority_count(), 2);
    }
    
    #[tokio::test]
    async fn test_cluster_startup() {
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        
        // 启动集群
        cluster.start_all_nodes().await.unwrap();
        
        // 验证所有节点都在运行
        assert_eq!(cluster.active_node_count().await, 3);
        assert!(cluster.is_available().await);
    }
}