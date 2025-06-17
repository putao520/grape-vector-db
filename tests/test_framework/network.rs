/// 网络模拟器
/// 
/// 用于模拟网络故障、分区、延迟等场景，支持故障注入测试

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use anyhow::Result;

/// 网络模拟器
pub struct NetworkSimulator {
    /// 当前分区状态
    partitions: Arc<RwLock<Vec<Partition>>>,
    /// 节点间延迟映射
    latency_map: Arc<RwLock<HashMap<(String, String), Duration>>>,
    /// 失败的节点集合
    failed_nodes: Arc<RwLock<HashSet<String>>>,
    /// 丢包率配置
    packet_loss_rates: Arc<RwLock<HashMap<String, f64>>>,
}

/// 网络分区
#[derive(Debug, Clone)]
pub struct Partition {
    pub id: String,
    pub nodes: HashSet<String>,
}

/// 网络混沌配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkChaos {
    /// 丢包率 (0.0-1.0)
    pub packet_loss: f64,
    /// 延迟尖峰 (毫秒)
    pub latency_spike: u64,
    /// 分区概率 (0.0-1.0)
    pub partition_probability: f64,
}

impl NetworkSimulator {
    /// 创建新的网络模拟器
    pub fn new() -> Self {
        Self {
            partitions: Arc::new(RwLock::new(Vec::new())),
            latency_map: Arc::new(RwLock::new(HashMap::new())),
            failed_nodes: Arc::new(RwLock::new(HashSet::new())),
            packet_loss_rates: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 创建网络分区（向后兼容）
    pub async fn create_partition_legacy(&self, partition1: Vec<String>, partition2: Vec<String>) {
        let mut partitions = self.partitions.write().await;
        
        // 清除现有分区
        partitions.clear();
        
        // 创建新分区
        if !partition1.is_empty() {
            partitions.push(Partition {
                id: "partition_1".to_string(),
                nodes: partition1.into_iter().collect(),
            });
        }
        
        if !partition2.is_empty() {
            partitions.push(Partition {
                id: "partition_2".to_string(),
                nodes: partition2.into_iter().collect(),
            });
        }
        
        tracing::info!("创建网络分区: {} 个分区", partitions.len());
    }
    
    /// 创建复杂分区（多个分区）
    pub async fn create_complex_partition(&self, partitions: Vec<Vec<String>>) {
        let mut current_partitions = self.partitions.write().await;
        current_partitions.clear();
        
        for (i, partition_nodes) in partitions.into_iter().enumerate() {
            if !partition_nodes.is_empty() {
                current_partitions.push(Partition {
                    id: format!("partition_{}", i),
                    nodes: partition_nodes.into_iter().collect(),
                });
            }
        }
        
        tracing::info!("创建复杂网络分区: {} 个分区", current_partitions.len());
    }
    
    /// 愈合所有分区
    pub async fn heal_all_partitions(&self) {
        let mut partitions = self.partitions.write().await;
        partitions.clear();
        tracing::info!("愈合所有网络分区");
    }
    
    /// 使用节点索引创建网络分区
    pub async fn create_partition(&self, group1: Vec<usize>, group2: Vec<usize>) -> Result<()> {
        let partition1: Vec<String> = group1.into_iter().map(|i| format!("node_{}", i)).collect();
        let partition2: Vec<String> = group2.into_iter().map(|i| format!("node_{}", i)).collect();
        self.create_partition_by_name(partition1, partition2).await;
        Ok(())
    }
    
    /// 使用节点名称创建网络分区
    pub async fn create_partition_by_name(&self, partition1: Vec<String>, partition2: Vec<String>) {
        let mut partitions = self.partitions.write().await;
        
        // 清除现有分区
        partitions.clear();
        
        // 创建新分区
        if !partition1.is_empty() {
            partitions.push(Partition {
                id: "partition_1".to_string(),
                nodes: partition1.into_iter().collect(),
            });
        }
        
        if !partition2.is_empty() {
            partitions.push(Partition {
                id: "partition_2".to_string(),
                nodes: partition2.into_iter().collect(),
            });
        }
        
        tracing::info!("创建网络分区: {} 个分区", partitions.len());
    }
    
    /// 修复网络分区
    pub async fn heal_partition(&self) -> Result<()> {
        self.heal_all_partitions().await;
        Ok(())
    }
    
    /// 检查两个节点是否可以通信
    pub async fn can_communicate(&self, node1: &str, node2: &str) -> bool {
        let partitions = self.partitions.read().await;
        let failed_nodes = self.failed_nodes.read().await;
        
        // 检查节点是否失败
        if failed_nodes.contains(node1) || failed_nodes.contains(node2) {
            return false;
        }
        
        // 如果没有分区，所有节点都可以通信
        if partitions.is_empty() {
            return true;
        }
        
        // 检查两个节点是否在同一个分区中
        for partition in partitions.iter() {
            if partition.nodes.contains(node1) && partition.nodes.contains(node2) {
                return true;
            }
        }
        
        false
    }
    
    /// 设置节点间延迟
    pub async fn set_latency(&self, node1: String, node2: String, latency: Duration) {
        let mut latency_map = self.latency_map.write().await;
        latency_map.insert((node1.clone(), node2.clone()), latency);
        latency_map.insert((node2, node1), latency);
    }
    
    /// 获取节点间延迟
    pub async fn get_latency(&self, node1: &str, node2: &str) -> Duration {
        let latency_map = self.latency_map.read().await;
        latency_map
            .get(&(node1.to_string(), node2.to_string()))
            .copied()
            .unwrap_or(Duration::from_millis(1)) // 默认1ms延迟
    }
    
    /// 模拟节点故障
    pub async fn fail_node(&self, node_id: String) {
        let mut failed_nodes = self.failed_nodes.write().await;
        failed_nodes.insert(node_id.clone());
        tracing::warn!("模拟节点故障: {}", node_id);
    }
    
    /// 恢复故障节点
    pub async fn recover_node(&self, node_id: &str) {
        let mut failed_nodes = self.failed_nodes.write().await;
        failed_nodes.remove(node_id);
        tracing::info!("恢复故障节点: {}", node_id);
    }
    
    /// 设置节点丢包率
    pub async fn set_packet_loss(&self, node_id: String, loss_rate: f64) {
        let mut packet_loss_rates = self.packet_loss_rates.write().await;
        packet_loss_rates.insert(node_id, loss_rate.clamp(0.0, 1.0));
    }
    
    /// 检查是否发生丢包
    pub async fn should_drop_packet(&self, node_id: &str) -> bool {
        let packet_loss_rates = self.packet_loss_rates.read().await;
        if let Some(&loss_rate) = packet_loss_rates.get(node_id) {
            fastrand::f64() < loss_rate
        } else {
            false
        }
    }
    
    /// 获取当前分区信息
    pub async fn get_partitions(&self) -> Vec<Partition> {
        self.partitions.read().await.clone()
    }
    
    /// 获取分区中的节点
    pub async fn get_nodes_in_partition(&self, partition_id: &str) -> Option<HashSet<String>> {
        let partitions = self.partitions.read().await;
        partitions
            .iter()
            .find(|p| p.id == partition_id)
            .map(|p| p.nodes.clone())
    }
    
    /// 获取最大分区（包含最多节点的分区）
    pub async fn get_largest_partition(&self) -> Option<Partition> {
        let partitions = self.partitions.read().await;
        partitions
            .iter()
            .max_by_key(|p| p.nodes.len())
            .cloned()
    }
    
    /// 检查网络是否健康（无分区且无故障节点）
    pub async fn is_network_healthy(&self) -> bool {
        let partitions = self.partitions.read().await;
        let failed_nodes = self.failed_nodes.read().await;
        partitions.is_empty() && failed_nodes.is_empty()
    }
    
    /// 获取网络统计信息
    pub async fn get_network_stats(&self) -> NetworkStats {
        let partitions = self.partitions.read().await;
        let failed_nodes = self.failed_nodes.read().await;
        let latency_map = self.latency_map.read().await;
        let packet_loss_rates = self.packet_loss_rates.read().await;
        
        NetworkStats {
            partition_count: partitions.len(),
            failed_node_count: failed_nodes.len(),
            total_latency_rules: latency_map.len(),
            nodes_with_packet_loss: packet_loss_rates.len(),
            average_latency_ms: if latency_map.is_empty() {
                0.0
            } else {
                latency_map.values().map(|d| d.as_millis() as f64).sum::<f64>() / latency_map.len() as f64
            },
        }
    }
    
    /// 注入网络混沌
    pub async fn inject_chaos(&self, chaos: NetworkChaos, duration: Duration) -> Result<()> {
        tracing::info!("开始注入网络混沌: {:?}", chaos);
        
        let start_time = std::time::Instant::now();
        
        while start_time.elapsed() < duration {
            // 随机选择操作
            let operation = fastrand::u32(0..3);
            
            match operation {
                0 => {
                    // 随机丢包
                    if fastrand::f64() < 0.1 { // 10%概率
                        let node_id = format!("node_{}", fastrand::u32(0..6));
                        self.set_packet_loss(node_id, chaos.packet_loss).await;
                    }
                }
                1 => {
                    // 延迟尖峰
                    if fastrand::f64() < 0.05 { // 5%概率
                        let node1 = format!("node_{}", fastrand::u32(0..6));
                        let node2 = format!("node_{}", fastrand::u32(0..6));
                        let spike_latency = Duration::from_millis(chaos.latency_spike);
                        self.set_latency(node1.clone(), node2.clone(), spike_latency).await;
                        
                        // 延迟一段时间后恢复
                        let simulator = self.clone();
                        let node1_clone = node1.clone();
                        let node2_clone = node2.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            simulator.set_latency(node1_clone, node2_clone, Duration::from_millis(1)).await;
                        });
                    }
                }
                2 => {
                    // 网络分区
                    if fastrand::f64() < chaos.partition_probability {
                        let partition1 = vec![
                            format!("node_{}", fastrand::u32(0..3)),
                            format!("node_{}", fastrand::u32(0..3)),
                        ];
                        let partition2 = vec![
                            format!("node_{}", fastrand::u32(3..6)),
                            format!("node_{}", fastrand::u32(3..6)),
                        ];
                        
                        self.create_partition_by_name(partition1, partition2).await;
                        
                        // 短时间后愈合分区
                        let simulator = self.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            simulator.heal_all_partitions().await;
                        });
                    }
                }
                _ => {}
            }
            
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        
        // 清理所有混沌效果
        self.heal_all_partitions().await;
        let mut packet_loss_rates = self.packet_loss_rates.write().await;
        packet_loss_rates.clear();
        
        tracing::info!("网络混沌注入结束");
        Ok(())
    }
}

/// 网络统计信息
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub partition_count: usize,
    pub failed_node_count: usize,
    pub total_latency_rules: usize,
    pub nodes_with_packet_loss: usize,
    pub average_latency_ms: f64,
}

impl Clone for NetworkSimulator {
    fn clone(&self) -> Self {
        Self {
            partitions: self.partitions.clone(),
            latency_map: self.latency_map.clone(),
            failed_nodes: self.failed_nodes.clone(),
            packet_loss_rates: self.packet_loss_rates.clone(),
        }
    }
}

/// 网络测试工具函数
pub mod network_test_utils {
    use super::*;
    
    /// 创建对称分区（每个分区节点数相等）
    pub async fn create_symmetric_partition(simulator: &NetworkSimulator, total_nodes: usize) {
        let half = total_nodes / 2;
        let partition1: Vec<String> = (0..half).map(|i| format!("node_{}", i)).collect();
        let partition2: Vec<String> = (half..total_nodes).map(|i| format!("node_{}", i)).collect();
        
        simulator.create_partition_by_name(partition1, partition2).await;
    }
    
    /// 创建多数派-少数派分区
    pub async fn create_majority_minority_partition(simulator: &NetworkSimulator, total_nodes: usize) {
        let minority_size = total_nodes / 3;
        let majority_size = total_nodes - minority_size;
        
        let minority: Vec<String> = (0..minority_size).map(|i| format!("node_{}", i)).collect();
        let majority: Vec<String> = (minority_size..total_nodes).map(|i| format!("node_{}", i)).collect();
        
        simulator.create_partition_by_name(minority, majority).await;
    }
    
    /// 随机失败节点
    pub async fn random_node_failures(simulator: &NetworkSimulator, node_count: usize, failure_rate: f64) {
        for i in 0..node_count {
            if fastrand::f64() < failure_rate {
                simulator.fail_node(format!("node_{}", i)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::network_test_utils::*;
    
    #[tokio::test]
    async fn test_network_simulator_creation() {
        let simulator = NetworkSimulator::new();
        assert!(simulator.is_network_healthy().await);
    }
    
    #[tokio::test]
    async fn test_partition_creation() {
        let simulator = NetworkSimulator::new();
        
        let partition1 = vec!["node1".to_string(), "node2".to_string()];
        let partition2 = vec!["node3".to_string(), "node4".to_string()];
        
        simulator.create_partition_by_name(partition1, partition2).await;
        
        // 验证同一分区内的节点可以通信
        assert!(simulator.can_communicate("node1", "node2").await);
        assert!(simulator.can_communicate("node3", "node4").await);
        
        // 验证不同分区间的节点无法通信
        assert!(!simulator.can_communicate("node1", "node3").await);
        assert!(!simulator.can_communicate("node2", "node4").await);
    }
    
    #[tokio::test]
    async fn test_node_failure() {
        let simulator = NetworkSimulator::new();
        
        simulator.fail_node("node1".to_string()).await;
        
        // 验证故障节点无法通信
        assert!(!simulator.can_communicate("node1", "node2").await);
        
        // 恢复节点
        simulator.recover_node("node1").await;
        
        // 验证恢复后可以通信
        assert!(simulator.can_communicate("node1", "node2").await);
    }
    
    #[tokio::test]
    async fn test_symmetric_partition() {
        let simulator = NetworkSimulator::new();
        
        create_symmetric_partition(&simulator, 6).await;
        
        let partitions = simulator.get_partitions().await;
        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions[0].nodes.len(), 3);
        assert_eq!(partitions[1].nodes.len(), 3);
    }
    
    #[tokio::test]
    async fn test_network_chaos() {
        let simulator = NetworkSimulator::new();
        
        let chaos = NetworkChaos {
            packet_loss: 0.1,
            latency_spike: 100,
            partition_probability: 0.2,
        };
        
        // 运行短时间的混沌测试
        simulator.inject_chaos(chaos, Duration::from_millis(100)).await.unwrap();
        
        // 验证混沌结束后网络恢复正常
        assert!(simulator.is_network_healthy().await);
    }
}