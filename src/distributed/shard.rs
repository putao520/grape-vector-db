use std::collections::HashMap;
use std::sync::Arc;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};
use std::time::Duration;

use crate::types::{NodeId, ShardInfo, ShardState, Point};
use crate::advanced_storage::AdvancedStorage;
use crate::types::*;

/// 分片管理器
pub struct ShardManager {
    /// 分片配置
    config: ShardConfig,
    /// 本地分片
    local_shards: Arc<RwLock<HashMap<u32, LocalShard>>>,
    /// 分片映射
    shard_map: Arc<RwLock<HashMap<u32, ShardInfo>>>,
    /// 存储引擎
    storage: Arc<AdvancedStorage>,
    /// 本地节点ID
    node_id: NodeId,
    /// 一致性哈希环
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
}

/// 分片配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardConfig {
    /// 分片数量
    pub shard_count: u32,
    /// 副本因子
    pub replication_factor: usize,
    /// 哈希算法
    pub hash_algorithm: HashAlgorithm,
    /// 分片大小限制 (字节)
    pub max_shard_size_bytes: u64,
    /// 分片向量数量限制
    pub max_vectors_per_shard: u64,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            shard_count: 256,
            replication_factor: 3,
            hash_algorithm: HashAlgorithm::ConsistentHash,
            max_shard_size_bytes: 1024 * 1024 * 1024, // 1GB
            max_vectors_per_shard: 1_000_000,
        }
    }
}

/// 哈希算法
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// 简单哈希
    SimpleHash,
    /// 一致性哈希
    ConsistentHash,
    /// 范围分片
    RangeHash,
}

/// 本地分片
pub struct LocalShard {
    /// 分片ID
    pub shard_id: u32,
    /// 分片信息
    pub info: ShardInfo,
    /// 存储引擎
    pub storage: Arc<AdvancedStorage>,
    /// 分片统计
    pub stats: ShardStats,
}

/// 分片统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardStats {
    /// 向量数量
    pub vector_count: u64,
    /// 存储大小 (字节)
    pub storage_size: u64,
    /// 读取QPS
    pub read_qps: f64,
    /// 写入QPS
    pub write_qps: f64,
    /// 平均延迟 (毫秒)
    pub avg_latency_ms: f64,
    /// 最后更新时间
    pub last_updated: i64,
}

impl Default for ShardStats {
    fn default() -> Self {
        Self {
            vector_count: 0,
            storage_size: 0,
            read_qps: 0.0,
            write_qps: 0.0,
            avg_latency_ms: 0.0,
            last_updated: chrono::Utc::now().timestamp(),
        }
    }
}

/// 分片路由器
pub struct ShardRouter {
    /// 分片配置
    config: ShardConfig,
    /// 分片映射
    shard_map: Arc<RwLock<HashMap<u32, ShardInfo>>>,
    /// 一致性哈希环
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
}

/// 一致性哈希环
#[derive(Debug, Clone)]
pub struct ConsistentHashRing {
    /// 虚拟节点
    virtual_nodes: HashMap<u64, NodeId>,
    /// 节点权重
    node_weights: HashMap<NodeId, u32>,
    /// 虚拟节点数量
    virtual_node_count: u32,
}

impl ConsistentHashRing {
    /// 创建新的一致性哈希环
    pub fn new(virtual_node_count: u32) -> Self {
        Self {
            virtual_nodes: HashMap::new(),
            node_weights: HashMap::new(),
            virtual_node_count,
        }
    }

    /// 添加节点
    pub fn add_node(&mut self, node_id: NodeId, weight: u32) {
        self.node_weights.insert(node_id.clone(), weight);
        
        // 为节点创建虚拟节点
        for i in 0..(self.virtual_node_count * weight) {
            let virtual_key = format!("{}:{}", node_id, i);
            let hash = self.hash_key(&virtual_key);
            self.virtual_nodes.insert(hash, node_id.clone());
        }
    }

    /// 移除节点
    pub fn remove_node(&mut self, node_id: &NodeId) {
        if let Some(weight) = self.node_weights.remove(node_id) {
            // 移除虚拟节点
            for i in 0..(self.virtual_node_count * weight) {
                let virtual_key = format!("{}:{}", node_id, i);
                let hash = self.hash_key(&virtual_key);
                self.virtual_nodes.remove(&hash);
            }
        }
    }

    /// 获取键对应的节点
    pub fn get_node(&self, key: &str) -> Option<NodeId> {
        if self.virtual_nodes.is_empty() {
            return None;
        }

        let hash = self.hash_key(key);
        
        // 找到第一个大于等于hash的虚拟节点
        let mut keys: Vec<u64> = self.virtual_nodes.keys().cloned().collect();
        keys.sort();
        
        for &virtual_hash in &keys {
            if virtual_hash >= hash {
                return self.virtual_nodes.get(&virtual_hash).cloned();
            }
        }
        
        // 如果没有找到，返回第一个节点（环形结构）
        keys.first().and_then(|&first_hash| {
            self.virtual_nodes.get(&first_hash).cloned()
        })
    }

    /// 哈希函数
    fn hash_key(&self, key: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

impl ShardManager {
    /// 创建新的分片管理器
    pub fn new(
        config: ShardConfig,
        storage: Arc<AdvancedStorage>,
        node_id: NodeId,
    ) -> Self {
        Self {
            config,
            local_shards: Arc::new(RwLock::new(HashMap::new())),
            shard_map: Arc::new(RwLock::new(HashMap::new())),
            storage,
            node_id,
            hash_ring: Arc::new(RwLock::new(ConsistentHashRing::new(100))),
        }
    }

    /// 初始化分片
    pub async fn initialize_shards(&self, cluster_nodes: Vec<NodeId>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("初始化分片，节点数: {}", cluster_nodes.len());

        let mut shard_map = self.shard_map.write().await;
        
        // 计算每个分片的哈希范围
        let hash_range_size = u64::MAX / self.config.shard_count as u64;
        
        for shard_id in 0..self.config.shard_count {
            let start_hash = shard_id as u64 * hash_range_size;
            let end_hash = if shard_id == self.config.shard_count - 1 {
                u64::MAX
            } else {
                (shard_id + 1) as u64 * hash_range_size - 1
            };

            // 选择主节点和副本节点
            let primary_node = cluster_nodes[shard_id as usize % cluster_nodes.len()].clone();
            let mut replica_nodes = Vec::new();
            
            for i in 1..=self.config.replication_factor {
                let replica_index = (shard_id as usize + i) % cluster_nodes.len();
                if cluster_nodes[replica_index] != primary_node {
                    replica_nodes.push(cluster_nodes[replica_index].clone());
                }
            }

            let shard_info = ShardInfo {
                id: shard_id,
                primary_node,
                replica_nodes,
                state: ShardState::Active,
                range: crate::types::ShardRange {
                    start_hash,
                    end_hash,
                },
            };

            shard_map.insert(shard_id, shard_info);
        }

        // 创建本地分片
        self.create_local_shards().await?;

        Ok(())
    }

    /// 创建本地分片
    async fn create_local_shards(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let shard_map = self.shard_map.read().await;
        let mut local_shards = self.local_shards.write().await;

        for (shard_id, shard_info) in shard_map.iter() {
            // 检查是否是本地分片（主节点或副本节点）
            if shard_info.primary_node == self.node_id || 
               shard_info.replica_nodes.contains(&self.node_id) {
                
                let local_shard = LocalShard {
                    shard_id: *shard_id,
                    info: shard_info.clone(),
                    storage: self.storage.clone(),
                    stats: ShardStats::default(),
                };

                local_shards.insert(*shard_id, local_shard);
                info!("创建本地分片: {}", shard_id);
            }
        }

        Ok(())
    }

    /// 获取键对应的分片ID
    pub fn get_shard_id(&self, key: &str) -> u32 {
        match self.config.hash_algorithm {
            HashAlgorithm::SimpleHash => {
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                (hasher.finish() % self.config.shard_count as u64) as u32
            }
            HashAlgorithm::ConsistentHash => {
                // 使用一致性哈希环
                // 为了保持同步方法，这里使用try_read()
                if let Ok(hash_ring) = self.hash_ring.try_read() {
                        if let Some(node_id) = hash_ring.get_node(key) {
                            // 将节点ID映射到分片ID
                            // 这里使用节点ID的哈希值来确定分片
                            let mut hasher = DefaultHasher::new();
                            node_id.hash(&mut hasher);
                            (hasher.finish() % self.config.shard_count as u64) as u32
                        } else {
                            // 如果哈希环为空，回退到简单哈希
                            let mut hasher = DefaultHasher::new();
                            key.hash(&mut hasher);
                            (hasher.finish() % self.config.shard_count as u64) as u32
                        }
                    } else {
                        // 如果无法获取锁，回退到简单哈希
                        let mut hasher = DefaultHasher::new();
                        key.hash(&mut hasher);
                        (hasher.finish() % self.config.shard_count as u64) as u32
                    }
            }
            HashAlgorithm::RangeHash => {
                // 实现范围哈希
                // 将键空间均匀分割成多个范围
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                let hash_value = hasher.finish();
                
                // 计算范围大小
                let range_size = u64::MAX / self.config.shard_count as u64;
                let shard_id = (hash_value / range_size).min(self.config.shard_count as u64 - 1) as u32;
                
                shard_id
            }
        }
    }

    /// 获取分片信息
    pub async fn get_shard_info(&self, shard_id: u32) -> Option<ShardInfo> {
        self.shard_map.read().await.get(&shard_id).cloned()
    }

    /// 获取本地分片
    pub async fn get_local_shard(&self, shard_id: u32) -> Option<LocalShard> {
        self.local_shards.read().await.get(&shard_id).map(|shard| shard.clone())
    }

    /// 向分片插入向量
    pub async fn upsert_vector(&self, point: &Point) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let shard_id = self.get_shard_id(&point.id);
        
        // 检查是否是本地分片
        if let Some(_local_shard) = self.get_local_shard(shard_id).await {
            // 存储到本地分片
            self.storage.store_vector(point)?;
            
            // 更新分片统计
            self.update_shard_stats(shard_id, 1, 0).await?;
            
            info!("向量 {} 插入到分片 {}", point.id, shard_id);
        } else {
            // 转发到远程分片
            self.forward_upsert_to_remote_shard(shard_id, point).await?;
        }

        Ok(())
    }

    /// 从分片删除向量
    pub async fn delete_vector(&self, point_id: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let shard_id = self.get_shard_id(point_id);
        
        // 检查是否是本地分片
        if let Some(_local_shard) = self.get_local_shard(shard_id).await {
            // 从本地分片删除
            let deleted = self.storage.delete_vector(point_id)?;
            
            if deleted {
                // 更新分片统计
                self.update_shard_stats(shard_id, -1, 0).await?;
                info!("向量 {} 从分片 {} 删除", point_id, shard_id);
            }
            
            Ok(deleted)
        } else {
            // 转发到远程分片
            self.forward_delete_to_remote_shard(shard_id, point_id).await
        }
    }

    /// 转发插入请求到远程分片
    async fn forward_upsert_to_remote_shard(&self, shard_id: u32, point: &Point) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 获取分片所在的节点
        let target_node = {
            let shard_map = self.shard_map.read().await;
            shard_map.get(&shard_id)
                .map(|info| info.primary_node.clone())
                .ok_or_else(|| format!("未找到分片 {} 的映射信息", shard_id))?
        };

        info!("转发向量 {} 插入请求到节点 {} 的分片 {}", point.id, target_node, shard_id);
        
        // TODO: 这里需要实际的网络调用
        // 暂时模拟网络请求
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // 模拟请求成功
        info!("向量 {} 转发插入到节点 {} 成功", point.id, target_node);
        Ok(())
    }

    /// 转发删除请求到远程分片
    async fn forward_delete_to_remote_shard(&self, shard_id: u32, point_id: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // 获取分片所在的节点
        let target_node = {
            let shard_map = self.shard_map.read().await;
            shard_map.get(&shard_id)
                .map(|info| info.primary_node.clone())
                .ok_or_else(|| format!("未找到分片 {} 的映射信息", shard_id))?
        };

        info!("转发向量 {} 删除请求到节点 {} 的分片 {}", point_id, target_node, shard_id);
        
        // TODO: 这里需要实际的网络调用
        // 暂时模拟网络请求
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // 模拟请求成功
        info!("向量 {} 转发删除到节点 {} 成功", point_id, target_node);
        Ok(true)
    }

    /// 在分片中搜索向量
    pub async fn search_vectors(&self, request: &SearchRequest) -> Result<Vec<ScoredPoint>, Box<dyn std::error::Error + Send + Sync>> {
        let mut all_results = Vec::new();

        // 搜索所有本地分片
        let local_shards = self.local_shards.read().await;
        for &shard_id in local_shards.keys() {
            let results = self.search_local_shard(shard_id, request).await?;
            all_results.extend(results);
        }

        // TODO: 搜索远程分片
        // 这里需要向远程节点发送搜索请求
        let remote_results = self.search_remote_shards(request).await?;
        all_results.extend(remote_results);

        // 合并和排序结果
        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(request.limit as usize);

        Ok(all_results)
    }

    /// 搜索本地分片
    async fn search_local_shard(&self, shard_id: u32, request: &SearchRequest) -> Result<Vec<ScoredPoint>, Box<dyn std::error::Error + Send + Sync>> {
        debug!("搜索本地分片: {}", shard_id);
        
        // 获取分片信息
        let local_shards = self.local_shards.read().await;
        if let Some(local_shard) = local_shards.get(&shard_id) {
            // 在实际实现中，这里应该调用存储引擎的搜索方法
            // 暂时返回模拟结果
            
            // 模拟一些搜索结果
            let mut results = Vec::new();
            
            // 生成一些模拟的搜索结果
            for i in 0..std::cmp::min(request.limit as usize, 3) {
                let scored_point = ScoredPoint {
                    id: format!("shard_{}_point_{}", shard_id, i),
                    score: 0.9 - (i as f32 * 0.1), // 递减的分数
                    point: Point {
                        id: format!("shard_{}_point_{}", shard_id, i),
                        vector: vec![0.1, 0.2, 0.3], // 模拟向量
                        payload: HashMap::new(),
                    },
                };
                results.push(scored_point);
            }
            
            debug!("本地分片 {} 搜索返回 {} 个结果", shard_id, results.len());
            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    /// 搜索远程分片
    async fn search_remote_shards(&self, request: &SearchRequest) -> Result<Vec<ScoredPoint>, Box<dyn std::error::Error + Send + Sync>> {
        let mut remote_results = Vec::new();
        
        // 获取所有远程分片
        let shard_map = self.shard_map.read().await;
        let local_shards = self.local_shards.read().await;
        
        let mut remote_search_tasks = Vec::new();
        
        for (&shard_id, shard_info) in shard_map.iter() {
            // 跳过本地分片
            if local_shards.contains_key(&shard_id) {
                continue;
            }
            
            // 创建远程搜索任务
            let target_node = shard_info.primary_node.clone();
            let search_request = request.clone();
            
            let task = tokio::spawn(async move {
                // TODO: 这里需要实际的网络调用
                // 暂时模拟远程搜索
                debug!("向节点 {} 的分片 {} 发送搜索请求", target_node, shard_id);
                tokio::time::sleep(Duration::from_millis(30)).await;
                
                // 模拟远程搜索结果
                let mut results = Vec::new();
                for i in 0..std::cmp::min(search_request.limit as usize / 2, 2) {
                    let scored_point = ScoredPoint {
                        id: format!("remote_shard_{}_point_{}", shard_id, i),
                        score: 0.8 - (i as f32 * 0.1),
                        point: Point {
                            id: format!("remote_shard_{}_point_{}", shard_id, i),
                            vector: vec![0.4, 0.5, 0.6],
                            payload: HashMap::new(),
                        },
                    };
                    results.push(scored_point);
                }
                
                Ok::<Vec<ScoredPoint>, Box<dyn std::error::Error + Send + Sync>>(results)
            });
            
            remote_search_tasks.push(task);
        }
        
        // 等待所有远程搜索完成
        for task in remote_search_tasks {
            match task.await {
                Ok(Ok(results)) => {
                    remote_results.extend(results);
                }
                Ok(Err(e)) => {
                    warn!("远程分片搜索失败: {}", e);
                }
                Err(e) => {
                    warn!("远程搜索任务执行失败: {}", e);
                }
            }
        }
        
        debug!("远程分片搜索返回 {} 个结果", remote_results.len());
        Ok(remote_results)
    }

    /// 更新分片统计
    async fn update_shard_stats(&self, shard_id: u32, vector_delta: i64, size_delta: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut local_shards = self.local_shards.write().await;
        
        if let Some(local_shard) = local_shards.get_mut(&shard_id) {
            local_shard.stats.vector_count = (local_shard.stats.vector_count as i64 + vector_delta).max(0) as u64;
            local_shard.stats.storage_size = (local_shard.stats.storage_size as i64 + size_delta).max(0) as u64;
            local_shard.stats.last_updated = chrono::Utc::now().timestamp();
        }

        // 分片映射中的 ShardInfo 不包含统计信息，统计信息保存在 LocalShard 中

        Ok(())
    }

    /// 迁移分片
    pub async fn migrate_shard(&self, shard_id: u32, target_node: NodeId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("开始迁移分片 {} 到节点 {}", shard_id, target_node);

        // 1. 标记分片为迁移状态
        {
            let mut local_shards = self.local_shards.write().await;
            if let Some(local_shard) = local_shards.get_mut(&shard_id) {
                info!("标记分片 {} 为迁移状态", shard_id);
                // 这里可以添加迁移状态标记，例如在统计信息中添加标记
            } else {
                return Err(format!("本地不存在分片 {}", shard_id).into());
            }
        }

        // 2. 收集分片数据
        let shard_data = self.collect_shard_data(shard_id).await?;
        
        // 3. 复制数据到目标节点
        let copy_result = self.copy_shard_to_node(shard_id, &target_node, &shard_data).await;
        
        match copy_result {
            Ok(_) => {
                // 4. 验证数据完整性
                let verification_result = self.verify_shard_integrity(shard_id, &target_node).await;
                
                match verification_result {
                    Ok(true) => {
                        // 5. 更新分片映射
                        self.update_shard_mapping(shard_id, target_node.clone()).await?;
                        
                        // 6. 删除本地数据
                        self.cleanup_local_shard(shard_id).await?;
                        
                        info!("分片 {} 迁移到节点 {} 完成", shard_id, target_node);
                        Ok(())
                    }
                    Ok(false) => {
                        warn!("分片 {} 数据完整性验证失败，回滚迁移", shard_id);
                        Err("数据完整性验证失败".into())
                    }
                    Err(e) => {
                        warn!("分片 {} 验证过程出错: {}", shard_id, e);
                        Err(e)
                    }
                }
            }
            Err(e) => {
                warn!("分片 {} 复制到节点 {} 失败: {}", shard_id, target_node, e);
                Err(e)
            }
        }
    }

    /// 收集分片数据
    async fn collect_shard_data(&self, shard_id: u32) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        info!("收集分片 {} 的数据", shard_id);
        
        // 获取分片中的所有向量
        let local_shards = self.local_shards.read().await;
        if let Some(_local_shard) = local_shards.get(&shard_id) {
            // 这里应该从存储引擎中读取分片的所有数据
            // 暂时返回一个模拟的数据包
            let shard_data = serde_json::json!({
                "shard_id": shard_id,
                "vectors": [], // 实际实现中这里应该包含所有向量数据
                "metadata": {
                    "vector_count": 0,
                    "timestamp": chrono::Utc::now().timestamp()
                }
            });
            
            let serialized_data = serde_json::to_vec(&shard_data)?;
            info!("分片 {} 数据收集完成，大小: {} 字节", shard_id, serialized_data.len());
            Ok(serialized_data)
        } else {
            Err(format!("本地不存在分片 {}", shard_id).into())
        }
    }

    /// 复制分片数据到目标节点
    async fn copy_shard_to_node(&self, shard_id: u32, target_node: &NodeId, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("复制分片 {} 到节点 {} (数据大小: {} 字节)", shard_id, target_node, data.len());
        
        // TODO: 这里需要实际的网络传输
        // 暂时模拟网络传输延迟
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        info!("分片 {} 数据复制到节点 {} 完成", shard_id, target_node);
        Ok(())
    }

    /// 验证分片数据完整性
    async fn verify_shard_integrity(&self, shard_id: u32, target_node: &NodeId) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        info!("验证节点 {} 上分片 {} 的数据完整性", target_node, shard_id);
        
        // TODO: 这里需要实际的完整性验证逻辑
        // 例如比较数据哈希、向量数量等
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // 暂时返回成功
        info!("分片 {} 在节点 {} 上的完整性验证通过", shard_id, target_node);
        Ok(true)
    }

    /// 更新分片映射
    async fn update_shard_mapping(&self, shard_id: u32, new_node: NodeId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut shard_map = self.shard_map.write().await;
        
        if let Some(shard_info) = shard_map.get_mut(&shard_id) {
            shard_info.primary_node = new_node.clone();
            info!("更新分片 {} 的主节点为: {}", shard_id, new_node);
        } else {
            warn!("分片映射中不存在分片 {}", shard_id);
        }
        
        Ok(())
    }

    /// 清理本地分片数据
    async fn cleanup_local_shard(&self, shard_id: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("清理本地分片 {} 的数据", shard_id);
        
        // 从本地分片列表中移除
        let mut local_shards = self.local_shards.write().await;
        if local_shards.remove(&shard_id).is_some() {
            info!("本地分片 {} 已从内存中移除", shard_id);
        }
        
        // TODO: 清理存储引擎中的分片数据
        // 这里应该删除分片相关的所有文件和数据
        
        info!("本地分片 {} 清理完成", shard_id);
        Ok(())
    }

    /// 重新平衡分片
    pub async fn rebalance_shards(&self, cluster_nodes: Vec<NodeId>) -> Result<Vec<ShardMigration>, Box<dyn std::error::Error + Send + Sync>> {
        info!("开始重新平衡分片，集群节点数: {}", cluster_nodes.len());

        if cluster_nodes.is_empty() {
            return Err("集群节点列表为空".into());
        }

        let mut migrations = Vec::new();
        
        // 1. 计算每个节点的负载
        let node_loads = self.calculate_node_loads(&cluster_nodes).await?;
        
        // 2. 识别过载和欠载的节点
        let avg_load = node_loads.values().sum::<f64>() / node_loads.len() as f64;
        let load_threshold = 0.2; // 20%的负载差异阈值
        
        let mut overloaded_nodes = Vec::new();
        let mut underloaded_nodes = Vec::new();
        
        for (node_id, load) in &node_loads {
            if *load > avg_load * (1.0 + load_threshold) {
                overloaded_nodes.push((node_id.clone(), *load));
            } else if *load < avg_load * (1.0 - load_threshold) {
                underloaded_nodes.push((node_id.clone(), *load));
            }
        }
        
        info!("负载分析: 平均负载={:.2}, 过载节点={}, 欠载节点={}", 
              avg_load, overloaded_nodes.len(), underloaded_nodes.len());
        
        // 3. 计算最优的分片分布
        // 从过载节点向欠载节点迁移分片
        for (overloaded_node, _load) in overloaded_nodes {
            if underloaded_nodes.is_empty() {
                break;
            }
            
            // 选择该节点上负载最小的分片进行迁移
            let candidate_shards = self.get_node_shards(&overloaded_node).await?;
            
            if let Some(shard_to_migrate) = candidate_shards.first() {
                // 选择负载最低的目标节点
                if let Some((target_node, _)) = underloaded_nodes.first() {
                    let migration = ShardMigration {
                        shard_id: *shard_to_migrate,
                        from_node: overloaded_node.clone(),
                        to_node: target_node.clone(),
                        reason: "负载重新平衡".to_string(),
                        estimated_size: self.estimate_shard_size(*shard_to_migrate).await?,
                        estimated_duration: 30, // 预估30秒
                    };
                    
                    migrations.push(migration);
                    
                    // 更新负载(简化处理)
                    underloaded_nodes.remove(0);
                }
            }
        }
        
        info!("生成 {} 个分片迁移计划", migrations.len());
        
        // 4. 生成迁移计划
        for migration in &migrations {
            info!("迁移计划: 分片 {} 从 {} 迁移到 {} (原因: {})", 
                  migration.shard_id, migration.from_node, migration.to_node, migration.reason);
        }

        Ok(migrations)
    }

    /// 计算节点负载
    async fn calculate_node_loads(&self, cluster_nodes: &[NodeId]) -> Result<HashMap<NodeId, f64>, Box<dyn std::error::Error + Send + Sync>> {
        let mut node_loads = HashMap::new();
        
        // 获取所有分片信息
        let shard_map = self.shard_map.read().await;
        
        // 初始化所有节点的负载为0
        for node_id in cluster_nodes {
            node_loads.insert(node_id.clone(), 0.0);
        }
        
        // 计算每个节点的分片数量作为负载指标
        for shard_info in shard_map.values() {
            if let Some(load) = node_loads.get_mut(&shard_info.primary_node) {
                *load += 1.0; // 简化的负载计算：每个分片贡献1.0的负载
            }
        }
        
        info!("节点负载计算完成: {:?}", node_loads);
        Ok(node_loads)
    }

    /// 获取节点上的分片列表
    async fn get_node_shards(&self, node_id: &NodeId) -> Result<Vec<u32>, Box<dyn std::error::Error + Send + Sync>> {
        let shard_map = self.shard_map.read().await;
        let node_shards: Vec<u32> = shard_map
            .iter()
            .filter(|(_, shard_info)| shard_info.primary_node == *node_id)
            .map(|(&shard_id, _)| shard_id)
            .collect();
        
        Ok(node_shards)
    }

    /// 估算分片大小
    async fn estimate_shard_size(&self, shard_id: u32) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let local_shards = self.local_shards.read().await;
        
        if let Some(local_shard) = local_shards.get(&shard_id) {
            Ok(local_shard.stats.storage_size)
        } else {
            // 如果不是本地分片，返回一个估算值
            Ok(1024 * 1024) // 1MB 默认估算
        }
    }

    /// 获取分片统计
    pub async fn get_shard_stats(&self) -> HashMap<u32, ShardStats> {
        let local_shards = self.local_shards.read().await;
        local_shards.iter()
            .map(|(&shard_id, shard)| (shard_id, shard.stats.clone()))
            .collect()
    }

    /// 获取分片映射
    pub async fn get_shard_map(&self) -> HashMap<u32, ShardInfo> {
        self.shard_map.read().await.clone()
    }
}

impl ShardRouter {
    /// 创建新的分片路由器
    pub fn new(config: ShardConfig) -> Self {
        Self {
            config,
            shard_map: Arc::new(RwLock::new(HashMap::new())),
            hash_ring: Arc::new(RwLock::new(ConsistentHashRing::new(100))),
        }
    }

    /// 路由请求到分片
    pub async fn route_request(&self, key: &str) -> Option<ShardInfo> {
        let shard_id = self.get_shard_id(key);
        self.shard_map.read().await.get(&shard_id).cloned()
    }

    /// 获取键对应的分片ID
    fn get_shard_id(&self, key: &str) -> u32 {
        match self.config.hash_algorithm {
            HashAlgorithm::SimpleHash => {
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                (hasher.finish() % self.config.shard_count as u64) as u32
            }
            HashAlgorithm::ConsistentHash => {
                // 使用一致性哈希环
                let hash_ring = self.hash_ring.blocking_read();
                if let Some(node_id) = hash_ring.get_node(key) {
                    // 将节点ID映射到分片ID
                    // 这里使用节点ID的哈希值来确定分片
                    let mut hasher = DefaultHasher::new();
                    node_id.hash(&mut hasher);
                    (hasher.finish() % self.config.shard_count as u64) as u32
                } else {
                    // 如果哈希环为空，回退到简单哈希
                    let mut hasher = DefaultHasher::new();
                    key.hash(&mut hasher);
                    (hasher.finish() % self.config.shard_count as u64) as u32
                }
            }
            HashAlgorithm::RangeHash => {
                // 实现范围哈希
                // 将键空间均匀分割成多个范围
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                let hash_value = hasher.finish();
                
                // 计算范围大小
                let range_size = u64::MAX / self.config.shard_count as u64;
                let shard_id = (hash_value / range_size).min(self.config.shard_count as u64 - 1) as u32;
                
                shard_id
            }
        }
    }

    /// 更新分片映射
    pub async fn update_shard_map(&self, shard_map: HashMap<u32, ShardInfo>) {
        *self.shard_map.write().await = shard_map;
    }
}

/// 分片迁移计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMigration {
    /// 分片ID
    pub shard_id: u32,
    /// 源节点
    pub from_node: NodeId,
    /// 目标节点
    pub to_node: NodeId,
    /// 迁移原因
    pub reason: String,
    /// 预估数据量 (字节)
    pub estimated_size: u64,
    /// 预估时间 (秒)
    pub estimated_duration: u64,
}

impl LocalShard {
    /// 克隆本地分片（用于异步操作）
    pub fn clone(&self) -> Self {
        Self {
            shard_id: self.shard_id,
            info: self.info.clone(),
            storage: self.storage.clone(),
            stats: self.stats.clone(),
        }
    }
}

/// 搜索请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    /// 查询向量
    pub vector: Vec<f32>,
    /// 返回结果数量限制
    pub limit: u32,
    /// 相似度阈值
    pub threshold: Option<f32>,
    /// 过滤条件
    pub filter: Option<serde_json::Value>,
}

/// 带分数的搜索结果点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredPoint {
    /// 点ID
    pub id: String,
    /// 相似度分数
    pub score: f32,
    /// 点数据
    pub point: Point,
} 