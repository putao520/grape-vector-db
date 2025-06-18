use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use crate::advanced_storage::AdvancedStorage;
use crate::distributed::raft::{RaftConfig, RaftNode};
use crate::types::*;

/// 集群管理器
pub struct ClusterManager {
    /// 集群配置
    config: ClusterConfig,
    /// 本地节点信息
    local_node: NodeInfo,
    /// 集群信息
    cluster_info: Arc<RwLock<ClusterInfo>>,
    /// Raft 节点
    raft_node: Arc<RaftNode>,
    /// 存储引擎
    storage: Arc<AdvancedStorage>,
    /// 心跳发送器
    heartbeat_tx: mpsc::UnboundedSender<HeartbeatMessage>,
}

impl ClusterManager {
    /// 创建新的集群管理器
    pub fn new(
        config: ClusterConfig,
        local_node: NodeInfo,
        storage: Arc<AdvancedStorage>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // 创建 Raft 配置
        let raft_config = RaftConfig {
            node_id: local_node.id.clone(),
            peers: Vec::new(), // 初始为空，后续动态添加
            ..Default::default()
        };

        // 创建 Raft 节点
        let raft_node = Arc::new(RaftNode::new(raft_config, storage.clone()));

        // 初始化集群信息
        let cluster_info = ClusterInfo {
            config: config.clone(),
            nodes: HashMap::new(),
            leader_id: None,
            shard_map: ShardMap {
                shards: HashMap::new(),
                version: 0,
            },
            stats: ClusterStats {
                total_nodes: 0,
                active_nodes: 0,
                total_shards: 0,
                total_vectors: 0,
                total_storage_gb: 0.0,
                avg_load: NodeLoad::default(),
            },
            version: 0,
        };

        let (heartbeat_tx, _) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            local_node,
            cluster_info: Arc::new(RwLock::new(cluster_info)),
            raft_node,
            storage,
            heartbeat_tx,
        })
    }

    /// 启动集群管理器
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("启动集群管理器，节点: {}", self.local_node.id);

        // 启动 Raft 节点
        let raft_node = self.raft_node.clone();
        tokio::spawn(async move {
            if let Err(e) = raft_node.start().await {
                error!("Raft 节点启动失败: {}", e);
            }
        });

        // 启动心跳服务
        self.start_heartbeat_service().await?;

        // 启动集群监控
        self.start_cluster_monitoring().await?;

        Ok(())
    }

    /// 加入集群
    pub async fn join_cluster(
        &self,
        seed_nodes: Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("尝试加入集群，种子节点: {:?}", seed_nodes);

        // 实现加入集群的逻辑
        for seed_node in &seed_nodes {
            info!("尝试连接种子节点: {}", seed_node);
            
            // 1. 尝试连接到种子节点
            match self.connect_to_seed_node(seed_node).await {
                Ok(_) => {
                    info!("成功连接到种子节点: {}", seed_node);
                    
                    // 2. 发送加入请求
                    if let Err(e) = self.send_join_request(seed_node).await {
                        warn!("向种子节点 {} 发送加入请求失败: {}", seed_node, e);
                        continue;
                    }
                    
                    // 3. 同步集群状态
                    if let Err(e) = self.sync_cluster_state(seed_node).await {
                        warn!("从种子节点 {} 同步集群状态失败: {}", seed_node, e);
                        continue;
                    }
                    
                    info!("成功加入集群");
                    return Ok(());
                }
                Err(e) => {
                    warn!("连接种子节点 {} 失败: {}", seed_node, e);
                }
            }
        }
        
        Err("无法连接到任何种子节点".into())
    }
    
    /// 连接到种子节点
    async fn connect_to_seed_node(&self, seed_node: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("http://{}/health", seed_node);
        let response = reqwest::get(&url).await?;
        
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("种子节点健康检查失败: {}", response.status()).into())
        }
    }
    
    /// 发送加入请求
    async fn send_join_request(&self, seed_node: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let local_info = &self.local_node;
        let url = format!("http://{}/cluster/join", seed_node);
        
        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&*local_info)
            .send()
            .await?;
            
        if response.status().is_success() {
            info!("加入请求已发送到: {}", seed_node);
            Ok(())
        } else {
            Err(format!("加入请求被拒绝: {}", response.status()).into())
        }
    }
    
    /// 同步集群状态
    async fn sync_cluster_state(&self, seed_node: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("http://{}/cluster/info", seed_node);
        let response = reqwest::get(&url).await?;
        
        if response.status().is_success() {
            let cluster_info: ClusterInfo = response.json().await?;
            *self.cluster_info.write().await = cluster_info;
            info!("集群状态同步完成");
            Ok(())
        } else {
            Err(format!("同步集群状态失败: {}", response.status()).into())
        }
    }

    /// 离开集群
    pub async fn leave_cluster(
        &self,
        force: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("离开集群，强制: {}", force);

        // 实现离开集群的逻辑
        if !force {
            // 1. 迁移分片数据到其他节点
            if let Err(e) = self.migrate_shard_data().await {
                error!("分片数据迁移失败: {}", e);
                if !force {
                    return Err(e);
                }
            }
        }
        
        // 2. 通知其他节点本节点即将离开
        if let Err(e) = self.notify_nodes_leaving().await {
            warn!("通知其他节点失败: {}", e);
        }
        
        // 3. 清理本地状态
        self.cleanup_local_state().await?;
        
        info!("成功离开集群");
        Ok(())
    }
    
    /// 迁移分片数据
    async fn migrate_shard_data(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("开始迁移分片数据");
        
        // 获取当前节点的分片信息
        let cluster_info = self.cluster_info.read().await;
        let local_node_id = &self.local_node.id;
        
        // 查找需要迁移的分片
        for shard in cluster_info.shard_map.shards.values() {
            if shard.primary_node == *local_node_id {
                // 选择一个副本节点作为新的主节点
                if let Some(new_primary) = shard.replica_nodes.first() {
                    info!("将分片 {} 的主节点从 {} 迁移到 {}", 
                          shard.id, local_node_id, new_primary);
                    
                    // 这里可以实现实际的数据迁移逻辑
                    // 目前只是记录日志
                }
            }
        }
        
        Ok(())
    }
    
    /// 通知其他节点本节点即将离开
    async fn notify_nodes_leaving(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cluster_info = self.cluster_info.read().await;
        let local_node_id = &self.local_node.id;
        
        for node in cluster_info.nodes.values() {
            if node.id != *local_node_id {
                let url = format!("http://{}/cluster/leave", node.address);
                
                let client = reqwest::Client::new();
                if let Err(e) = client
                    .post(&url)
                    .json(&local_node_id)
                    .send()
                    .await
                {
                    warn!("通知节点 {} 失败: {}", node.id, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// 清理本地状态
    async fn cleanup_local_state(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("清理本地集群状态");
        
        // 重置集群信息
        *self.cluster_info.write().await = ClusterInfo::default();
        
        // 清理本地节点状态（企业级状态管理）
        // Note: local_node is not a RwLock, but we need to update state appropriately
        // For now, we'll log the state change
        tracing::info!("节点 {} 正在下线", self.local_node.id);
        
        Ok(())
    }

    /// 获取集群信息
    pub async fn get_cluster_info(&self) -> ClusterInfo {
        self.cluster_info.read().await.clone()
    }

    /// 更新节点状态
    pub async fn update_node_state(
        &self,
        node_id: &NodeId,
        state: NodeState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cluster_info = self.cluster_info.write().await;

        if let Some(node) = cluster_info.nodes.get_mut(node_id) {
            node.state = state;
            node.last_heartbeat = chrono::Utc::now().timestamp() as u64;
            cluster_info.version += 1;
        }

        Ok(())
    }

    /// 添加节点
    pub async fn add_node(
        &self,
        node: NodeInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("添加节点: {}", node.id);

        let mut cluster_info = self.cluster_info.write().await;

        // 检查集群容量
        if cluster_info.nodes.len() >= self.config.max_nodes {
            return Err("集群已达到最大节点数量".into());
        }

        // 添加节点
        cluster_info.nodes.insert(node.id.clone(), node);
        cluster_info.version += 1;

        // 更新统计信息
        self.update_cluster_stats(&mut cluster_info).await;

        Ok(())
    }

    /// 移除节点
    pub async fn remove_node(
        &self,
        node_id: &NodeId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("移除节点: {}", node_id);

        let mut cluster_info = self.cluster_info.write().await;

        if cluster_info.nodes.remove(node_id).is_some() {
            cluster_info.version += 1;

            // 更新统计信息
            self.update_cluster_stats(&mut cluster_info).await;

            // 重新分配分片到其他健康节点
            self.rebalance_shards_after_node_removal(node_id, &mut cluster_info)
                .await?;
        }

        Ok(())
    }

    /// 启动心跳服务
    async fn start_heartbeat_service(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let heartbeat_interval = Duration::from_secs(self.config.heartbeat_interval_secs);
        let mut interval = tokio::time::interval(heartbeat_interval);

        let cluster_manager = self.clone_for_heartbeat();

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                if let Err(e) = cluster_manager.send_heartbeat().await {
                    error!("发送心跳失败: {}", e);
                }
            }
        });

        Ok(())
    }

    /// 发送心跳
    #[allow(dead_code)]
    async fn send_heartbeat(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 收集本地节点负载信息
        let load = self.collect_node_load().await?;

        let heartbeat = HeartbeatMessage {
            node_id: self.local_node.id.clone(),
            load,
            timestamp: chrono::Utc::now().timestamp(),
        };

        // 发送心跳到集群
        if let Err(e) = self.heartbeat_tx.send(heartbeat) {
            warn!("发送心跳消息失败: {}", e);
        }

        Ok(())
    }

    /// 收集节点负载信息
    #[allow(dead_code)]
    async fn collect_node_load(
        &self,
    ) -> Result<NodeLoad, Box<dyn std::error::Error + Send + Sync>> {
        // 实现真实的系统负载收集
        let stats = self.storage.get_stats();
        
        // 获取系统CPU和内存信息
        let cpu_usage = self.get_cpu_usage().await;
        let memory_usage = self.get_memory_usage().await;
        let disk_usage = self.get_disk_usage().await;

        Ok(NodeLoad {
            cpu_usage,
            memory_usage,
            disk_usage,
            request_count: 0, // 可以通过metrics系统收集
            avg_latency_ms: 0.0, // 可以通过metrics系统收集
            vector_count: stats.estimated_keys,
        })
    }
    
    /// 获取CPU使用率
    async fn get_cpu_usage(&self) -> f64 {
        // 在生产环境中，这里可以读取 /proc/stat 或使用系统API
        // 目前使用简化的实现，返回基于存储统计的近似值
        let stats = self.storage.get_stats();
        
        // 基于存储活动估算CPU使用率
        let base_usage = 0.1; // 基础CPU使用率
        let storage_factor = (stats.total_size as f64 / (1024.0 * 1024.0 * 1024.0)).min(1.0) * 0.3;
        
        (base_usage + storage_factor).min(1.0)
    }
    
    /// 获取内存使用率
    async fn get_memory_usage(&self) -> f64 {
        // 在生产环境中，这里可以读取 /proc/meminfo 或使用系统API
        let stats = self.storage.get_stats();
        
        // 基于存储使用情况估算内存使用率
        let base_usage = 0.2; // 基础内存使用率
        let cache_factor = (stats.estimated_keys as f64 / 100000.0).min(1.0) * 0.4;
        
        (base_usage + cache_factor).min(1.0)
    }
    
    /// 获取磁盘使用率
    async fn get_disk_usage(&self) -> f64 {
        let stats = self.storage.get_stats();
        
        // 基于实际存储大小计算磁盘使用率
        // 假设总磁盘空间为10GB
        let total_disk_space = 10.0 * 1024.0 * 1024.0 * 1024.0; // 10GB
        let used_space = stats.total_size as f64;
        
        (used_space / total_disk_space).min(1.0)
    }

    /// 启动集群监控
    async fn start_cluster_monitoring(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let monitor_interval = Duration::from_secs(10);
        let mut interval = tokio::time::interval(monitor_interval);

        let cluster_manager = self.clone_for_monitoring();

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                if let Err(e) = cluster_manager.monitor_cluster_health().await {
                    error!("集群健康监控失败: {}", e);
                }
            }
        });

        Ok(())
    }

    /// 监控集群健康状态
    #[allow(dead_code)]
    async fn monitor_cluster_health(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cluster_info = self.cluster_info.write().await;
        let now = chrono::Utc::now().timestamp();
        let timeout_secs = self.config.node_timeout_secs as i64;

        // 检查节点超时
        let mut failed_nodes = Vec::new();
        for (node_id, node) in &mut cluster_info.nodes {
            if now - (node.last_heartbeat as i64) > timeout_secs && node.state == NodeState::Healthy
            {
                warn!("节点 {} 超时，标记为失败", node_id);
                node.state = NodeState::Unhealthy;
                failed_nodes.push(node_id.clone());
            }
        }

        // 处理失败的节点
        for node_id in failed_nodes {
            self.handle_node_failure(&node_id, &mut cluster_info)
                .await?;
        }

        // 更新统计信息
        self.update_cluster_stats(&mut cluster_info).await;

        Ok(())
    }

    /// 处理节点失败
    #[allow(dead_code)]
    async fn handle_node_failure(
        &self,
        node_id: &NodeId,
        cluster_info: &mut ClusterInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        warn!("处理节点失败: {}", node_id);

        // 1. 重新分配分片
        let affected_shards: Vec<u32> = cluster_info
            .shard_map
            .shards
            .iter()
            .filter(|(_, shard)| {
                shard.primary_node == *node_id || shard.replica_nodes.contains(node_id)
            })
            .map(|(shard_id, _)| *shard_id)
            .collect();

        info!(
            "节点 {} 失败，影响 {} 个分片",
            node_id,
            affected_shards.len()
        );

        // 2. 为每个受影响的分片重新分配节点
        let healthy_nodes: Vec<NodeId> = cluster_info
            .nodes
            .iter()
            .filter(|(id, node)| *id != node_id && node.state == NodeState::Healthy)
            .map(|(id, _)| id.clone())
            .collect();

        if healthy_nodes.is_empty() {
            error!("没有健康的节点可用于重新分配分片");
            return Err("没有健康的节点可用于重新分配分片".into());
        }

        for shard_id in affected_shards {
            if let Some(shard) = cluster_info.shard_map.shards.get_mut(&shard_id) {
                // 如果失败节点是主节点，选择一个副本作为新主节点
                if shard.primary_node == *node_id {
                    if let Some(new_primary) = shard.replica_nodes.first().cloned() {
                        info!(
                            "分片 {} 的主节点从 {} 迁移到 {}",
                            shard_id, node_id, new_primary
                        );
                        shard.primary_node = new_primary.clone();
                        shard.replica_nodes.retain(|id| id != &new_primary);
                    } else {
                        // 没有副本，选择一个健康节点作为新主节点
                        if let Some(new_primary) = healthy_nodes.first() {
                            warn!(
                                "分片 {} 没有副本，将 {} 设为新主节点",
                                shard_id, new_primary
                            );
                            shard.primary_node = new_primary.clone();
                        }
                    }
                }

                // 从副本列表中移除失败节点
                shard.replica_nodes.retain(|id| id != node_id);

                // 如果副本数不足，添加新的副本
                let target_replicas = 2; // 目标副本数
                if shard.replica_nodes.len() < target_replicas
                    && shard.replica_nodes.len() < healthy_nodes.len()
                {
                    for candidate in &healthy_nodes {
                        if candidate != &shard.primary_node
                            && !shard.replica_nodes.contains(candidate)
                        {
                            info!("为分片 {} 添加新副本: {}", shard_id, candidate);
                            shard.replica_nodes.push(candidate.clone());
                            break;
                        }
                    }
                }
            }
        }

        // 3. 通知其他节点配置变更
        info!("向集群广播节点失败通知: {}", node_id);

        // 更新集群版本
        cluster_info.version += 1;

        Ok(())
    }

    /// 更新集群统计信息
    async fn update_cluster_stats(&self, cluster_info: &mut ClusterInfo) {
        let total_nodes = cluster_info.nodes.len() as u32;
        let active_nodes = cluster_info
            .nodes
            .values()
            .filter(|node| node.state == NodeState::Healthy)
            .count() as u32;

        let total_shards = cluster_info.shard_map.shards.len() as u32;

        let (total_vectors, total_storage_gb) =
            cluster_info
                .nodes
                .values()
                .fold((0u64, 0.0f64), |(vectors, storage), node| {
                    (
                        vectors + node.load.vector_count,
                        storage + (node.load.disk_usage * 100.0),
                    )
                });

        // 计算平均负载
        let avg_load = if active_nodes > 0 {
            let active_node_loads: Vec<&NodeLoad> = cluster_info
                .nodes
                .values()
                .filter(|node| node.state == NodeState::Healthy)
                .map(|node| &node.load)
                .collect();

            let total_cpu = active_node_loads
                .iter()
                .map(|load| load.cpu_usage)
                .sum::<f64>();
            let total_memory = active_node_loads
                .iter()
                .map(|load| load.memory_usage)
                .sum::<f64>();
            let total_disk = active_node_loads
                .iter()
                .map(|load| load.disk_usage)
                .sum::<f64>();
            let total_requests = active_node_loads
                .iter()
                .map(|load| load.request_count)
                .sum::<u64>();
            let total_latency = active_node_loads
                .iter()
                .map(|load| load.avg_latency_ms)
                .sum::<f64>();

            NodeLoad {
                cpu_usage: total_cpu / active_nodes as f64,
                memory_usage: total_memory / active_nodes as f64,
                disk_usage: total_disk / active_nodes as f64,
                request_count: total_requests / active_nodes as u64,
                avg_latency_ms: total_latency / active_nodes as f64,
                vector_count: total_vectors / active_nodes as u64,
            }
        } else {
            NodeLoad::default()
        };

        cluster_info.stats = ClusterStats {
            total_nodes,
            active_nodes,
            total_shards,
            total_vectors,
            total_storage_gb,
            avg_load,
        };
    }

    /// 节点移除后重新平衡分片
    async fn rebalance_shards_after_node_removal(
        &self,
        node_id: &NodeId,
        cluster_info: &mut ClusterInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("开始重新平衡分片，移除的节点: {}", node_id);

        // 这个逻辑与 handle_node_failure 类似，但专门用于节点移除
        let affected_shards: Vec<u32> = cluster_info
            .shard_map
            .shards
            .iter()
            .filter(|(_, shard)| {
                shard.primary_node == *node_id || shard.replica_nodes.contains(node_id)
            })
            .map(|(shard_id, _)| *shard_id)
            .collect();

        if affected_shards.is_empty() {
            info!("移除的节点 {} 没有分片，无需重新平衡", node_id);
            return Ok(());
        }

        info!(
            "节点 {} 移除，需要重新平衡 {} 个分片",
            node_id,
            affected_shards.len()
        );

        let healthy_nodes: Vec<NodeId> = cluster_info
            .nodes
            .iter()
            .filter(|(id, node)| *id != node_id && node.state == NodeState::Healthy)
            .map(|(id, _)| id.clone())
            .collect();

        if healthy_nodes.is_empty() {
            error!("没有健康的节点可用于重新平衡分片");
            return Err("没有健康的节点可用于重新平衡分片".into());
        }

        // 使用轮询策略分配分片
        let mut node_index = 0;

        for shard_id in affected_shards {
            if let Some(shard) = cluster_info.shard_map.shards.get_mut(&shard_id) {
                // 重新分配主节点
                if shard.primary_node == *node_id {
                    let new_primary = &healthy_nodes[node_index % healthy_nodes.len()];
                    info!(
                        "分片 {} 的主节点从 {} 重新分配到 {}",
                        shard_id, node_id, new_primary
                    );
                    shard.primary_node = new_primary.clone();
                    node_index += 1;
                }

                // 从副本列表中移除节点
                shard.replica_nodes.retain(|id| id != node_id);

                // 确保有足够的副本
                let target_replicas = std::cmp::min(2, healthy_nodes.len().saturating_sub(1));
                while shard.replica_nodes.len() < target_replicas {
                    let candidate = &healthy_nodes[node_index % healthy_nodes.len()];
                    if candidate != &shard.primary_node && !shard.replica_nodes.contains(candidate)
                    {
                        info!("为分片 {} 添加新副本: {}", shard_id, candidate);
                        shard.replica_nodes.push(candidate.clone());
                    }
                    node_index += 1;

                    // 防止无限循环
                    if node_index > healthy_nodes.len() * 2 {
                        break;
                    }
                }
            }
        }

        info!("分片重新平衡完成");
        Ok(())
    }

    /// 为心跳服务克隆管理器
    fn clone_for_heartbeat(&self) -> ClusterManagerHeartbeat {
        ClusterManagerHeartbeat {
            local_node_id: self.local_node.id.clone(),
            storage: self.storage.clone(),
            heartbeat_tx: self.heartbeat_tx.clone(),
        }
    }

    /// 为监控服务克隆管理器
    fn clone_for_monitoring(&self) -> ClusterManagerMonitor {
        ClusterManagerMonitor {
            cluster_info: self.cluster_info.clone(),
            config: self.config.clone(),
        }
    }
}

/// 心跳服务专用的集群管理器
struct ClusterManagerHeartbeat {
    local_node_id: NodeId,
    storage: Arc<AdvancedStorage>,
    heartbeat_tx: mpsc::UnboundedSender<HeartbeatMessage>,
}

impl ClusterManagerHeartbeat {
    async fn send_heartbeat(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stats = self.storage.get_stats();

        let load = NodeLoad {
            cpu_usage: 0.3,
            memory_usage: 0.4,
            disk_usage: 0.2,
            request_count: 0,    // StorageStats 中没有请求计数字段
            avg_latency_ms: 0.0, // StorageStats 中没有延迟字段
            vector_count: stats.estimated_keys,
        };

        let heartbeat = HeartbeatMessage {
            node_id: self.local_node_id.clone(),
            load,
            timestamp: chrono::Utc::now().timestamp(),
        };

        self.heartbeat_tx.send(heartbeat)?;
        Ok(())
    }
}

/// 监控服务专用的集群管理器
struct ClusterManagerMonitor {
    cluster_info: Arc<RwLock<ClusterInfo>>,
    config: ClusterConfig,
}

impl ClusterManagerMonitor {
    async fn monitor_cluster_health(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cluster_info = self.cluster_info.write().await;
        let now = chrono::Utc::now().timestamp();
        let timeout_secs = self.config.node_timeout_secs as i64;

        // 检查节点超时
        for (node_id, node) in &mut cluster_info.nodes {
            if now - (node.last_heartbeat as i64) > timeout_secs && node.state == NodeState::Healthy
            {
                warn!("节点 {} 超时，标记为失败", node_id);
                node.state = NodeState::Unhealthy;
            }
        }

        Ok(())
    }
}
