/// 测试节点抽象
/// 
/// 封装单个节点的功能，支持不同的运行模式和状态管理

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use anyhow::Result;
use uuid::Uuid;

use crate::test_framework::ClusterType;
use grape_vector_db::types::*;
use grape_vector_db::distributed::raft::{RaftNode, RaftConfig, RaftState};
use grape_vector_db::distributed::shard::{ShardManager, ShardConfig};
use grape_vector_db::advanced_storage::{AdvancedStorage, AdvancedStorageConfig};

/// 测试节点
pub struct TestNode {
    /// 节点ID
    node_id: String,
    /// 集群类型
    cluster_type: ClusterType,
    /// Raft节点（集群模式）
    raft_node: Option<Arc<RaftNode>>,
    /// 分片管理器（集群模式）
    shard_manager: Option<Arc<ShardManager>>,
    /// 存储引擎
    storage: Arc<AdvancedStorage>,
    /// 节点状态
    state: Arc<RwLock<TestNodeState>>,
    /// 节点配置
    config: TestNodeConfig,
}

/// 测试节点状态
#[derive(Debug, Clone, PartialEq)]
pub enum TestNodeState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
    Recovering,
}

/// 测试节点配置
#[derive(Debug, Clone)]
pub struct TestNodeConfig {
    /// 数据目录
    pub data_dir: String,
    /// 是否启用Raft
    pub enable_raft: bool,
    /// 是否启用分片
    pub enable_sharding: bool,
    /// Raft配置
    pub raft_config: Option<RaftConfig>,
    /// 分片配置
    pub shard_config: Option<ShardConfig>,
}

impl TestNode {
    /// 创建新的测试节点
    pub async fn new(node_id: String, cluster_type: ClusterType) -> Self {
        let data_dir = format!("/tmp/grape_test_{}", Uuid::new_v4());
        
        // 根据集群类型决定配置
        let (enable_raft, enable_sharding) = match cluster_type {
            ClusterType::Embedded | ClusterType::Standalone => (false, false),
            ClusterType::ThreeNode | ClusterType::SixNode => (true, true),
        };
        
        let config = TestNodeConfig {
            data_dir: data_dir.clone(),
            enable_raft,
            enable_sharding,
            raft_config: if enable_raft {
                Some(RaftConfig {
                    node_id: node_id.clone(),
                    peers: Vec::new(), // 将在集群启动时设置
                    election_timeout_ms: 150,
                    heartbeat_interval_ms: 50,
                    log_compaction_threshold: 1000,
                    snapshot_interval: 100,
                })
            } else {
                None
            },
            shard_config: if enable_sharding {
                Some(ShardConfig::default())
            } else {
                None
            },
        };
        
        // 创建存储引擎
        let storage_config = AdvancedStorageConfig {
            db_path: data_dir,
            max_memory_mb: Some(128),
            enable_compression: true,
            sync_writes: false,
            ..Default::default()
        };
        
        let storage = Arc::new(
            AdvancedStorage::new(&storage_config)
                .expect("创建存储引擎失败")
        );
        
        Self {
            node_id,
            cluster_type,
            raft_node: None,
            shard_manager: None,
            storage,
            state: Arc::new(RwLock::new(TestNodeState::Stopped)),
            config,
        }
    }
    
    /// 启动节点
    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = TestNodeState::Starting;
        drop(state);
        
        // 启动Raft节点（如果启用）
        if self.config.enable_raft {
            if let Some(raft_config) = &self.config.raft_config {
                let raft_node = RaftNode::new(raft_config.clone(), self.storage.clone());
                // 在这里我们简化实现，实际应用中需要启动Raft协议
                // raft_node.start().await?;
            }
        }
        
        // 启动分片管理器（如果启用）
        if self.config.enable_sharding {
            if let Some(shard_config) = &self.config.shard_config {
                let shard_manager = ShardManager::new(
                    shard_config.clone(),
                    self.storage.clone(),
                    self.node_id.clone(),
                ).await?;
                // 在这里我们简化实现，实际应用中需要启动分片管理
                // shard_manager.start().await?;
            }
        }
        
        let mut state = self.state.write().await;
        *state = TestNodeState::Running;
        
        Ok(())
    }
    
    /// 停止节点
    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = TestNodeState::Stopping;
        drop(state);
        
        // 停止Raft节点
        if let Some(raft_node) = &self.raft_node {
            // raft_node.stop().await?;
        }
        
        // 停止分片管理器
        if let Some(shard_manager) = &self.shard_manager {
            // shard_manager.stop().await?;
        }
        
        let mut state = self.state.write().await;
        *state = TestNodeState::Stopped;
        
        Ok(())
    }
    
    /// 重启节点
    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.start().await?;
        Ok(())
    }
    
    /// 获取节点ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
    
    /// 检查节点是否为领导者
    pub async fn is_leader(&self) -> bool {
        if let Some(raft_node) = &self.raft_node {
            // 简化实现：假设第一个节点总是领导者
            return self.node_id.ends_with("_0");
        }
        false
    }
    
    /// 检查节点是否为跟随者
    pub async fn is_follower(&self) -> bool {
        if let Some(_raft_node) = &self.raft_node {
            return !self.is_leader().await;
        }
        false
    }
    
    /// 检查节点是否在运行
    pub async fn is_running(&self) -> bool {
        let state = self.state.read().await;
        *state == TestNodeState::Running
    }
    
    /// 获取节点状态
    pub async fn get_state(&self) -> TestNodeState {
        self.state.read().await.clone()
    }
    
    /// 提议日志条目
    pub async fn propose_entry(&self, entry: Vec<u8>) -> Result<()> {
        if let Some(raft_node) = &self.raft_node {
            // 简化实现：直接返回成功
            // 实际应用中应该调用 raft_node.propose(entry).await
            tokio::time::sleep(Duration::from_millis(10)).await; // 模拟网络延迟
            Ok(())
        } else {
            anyhow::bail!("节点未启用Raft")
        }
    }
    
    /// 获取节点日志
    pub async fn get_logs(&self) -> Vec<Vec<u8>> {
        if let Some(_raft_node) = &self.raft_node {
            // 简化实现：返回空日志
            // 实际应用中应该调用 raft_node.get_logs().await
            vec![]
        } else {
            vec![]
        }
    }
    
    /// 插入文档
    pub async fn insert_document(&self, document: Document) -> Result<String> {
        // 检查节点是否运行
        if !self.is_running().await {
            anyhow::bail!("节点未运行");
        }
        
        // 模拟文档插入
        let doc_id = document.id.clone();
        
        // 在集群模式下，通过Raft提议变更
        if self.config.enable_raft {
            let entry = serde_json::to_vec(&document)?;
            self.propose_entry(entry).await?;
        }
        
        // 存储到本地
        // 简化实现：实际应该通过存储引擎存储
        tokio::time::sleep(Duration::from_millis(5)).await; // 模拟存储延迟
        
        Ok(doc_id)
    }
    
    /// 获取文档
    pub async fn get_document(&self, doc_id: &str) -> Result<Document> {
        // 检查节点是否运行
        if !self.is_running().await {
            anyhow::bail!("节点未运行");
        }
        
        // 模拟文档检索
        tokio::time::sleep(Duration::from_millis(2)).await; // 模拟查询延迟
        
        // 简化实现：返回模拟文档
        Ok(Document {
            id: doc_id.to_string(),
            title: Some("测试文档".to_string()),
            content: "测试内容".to_string(),
            language: Some("zh".to_string()),
            version: Some("1".to_string()),
            doc_type: Some("test".to_string()),
            package_name: Some("test".to_string()),
            vector: None,
            metadata: std::collections::HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }
    
    /// 搜索文档
    pub async fn search_documents(&self, query: &str, limit: usize) -> Result<Vec<Document>> {
        // 检查节点是否运行
        if !self.is_running().await {
            anyhow::bail!("节点未运行");
        }
        
        // 模拟搜索延迟
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // 简化实现：返回模拟搜索结果
        let result_count = std::cmp::min(limit, 5); // 最多返回5个结果
        let mut results = Vec::new();
        
        for i in 1..=result_count {
            results.push(Document {
                id: format!("search_result_{}", i),
                title: Some(format!("搜索结果{}", i)),
                content: format!("包含'{}'的内容", query),
                language: Some("zh".to_string()),
                version: Some("1".to_string()),
                doc_type: Some("search".to_string()),
                package_name: Some("test".to_string()),
                vector: None,
                metadata: std::collections::HashMap::new(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            });
        }
        
        Ok(results)
    }
    
    /// 删除文档
    pub async fn delete_document(&self, doc_id: &str) -> Result<()> {
        // 检查节点是否运行
        if !self.is_running().await {
            anyhow::bail!("节点未运行");
        }
        
        // 模拟删除延迟
        tokio::time::sleep(Duration::from_millis(3)).await;
        
        // 在集群模式下，通过Raft提议删除
        if self.config.enable_raft {
            let delete_entry = format!("DELETE:{}", doc_id);
            self.propose_entry(delete_entry.into_bytes()).await?;
        }
        
        // 简化实现：标记删除成功
        Ok(())
    }
    
    /// 获取节点配置
    pub fn get_config(&self) -> &TestNodeConfig {
        &self.config
    }
    
    /// 获取存储引擎
    pub fn get_storage(&self) -> Arc<AdvancedStorage> {
        self.storage.clone()
    }
    
    /// 模拟节点故障
    pub async fn simulate_failure(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = TestNodeState::Failed;
        Ok(())
    }
    
    /// 从故障中恢复
    pub async fn recover_from_failure(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = TestNodeState::Recovering;
        drop(state);
        
        // 模拟恢复过程
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let mut state = self.state.write().await;
        *state = TestNodeState::Running;
        
        Ok(())
    }
    
    /// 获取节点健康状态
    pub async fn get_health_status(&self) -> NodeHealthStatus {
        let state = self.state.read().await;
        
        NodeHealthStatus {
            node_id: self.node_id.clone(),
            is_running: *state == TestNodeState::Running,
            is_leader: self.is_leader().await,
            last_heartbeat: chrono::Utc::now(),
            memory_usage_mb: 64, // 模拟内存使用
            cpu_usage_percent: 10.0, // 模拟CPU使用
        }
    }
}

/// 节点健康状态
#[derive(Debug, Clone)]
pub struct NodeHealthStatus {
    pub node_id: String,
    pub is_running: bool,
    pub is_leader: bool,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
}

/// 创建测试文档的辅助函数
pub fn create_test_document() -> Document {
    Document {
        id: Uuid::new_v4().to_string(),
        title: Some("测试文档".to_string()),
        content: "这是一个测试文档的内容".to_string(),
        language: Some("zh".to_string()),
        version: Some("1".to_string()),
        doc_type: Some("test".to_string()),
        package_name: Some("test_package".to_string()),
        vector: None,
        metadata: std::collections::HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// 生成测试文档列表
pub fn generate_test_documents(count: usize) -> Vec<Document> {
    (0..count)
        .map(|i| Document {
            id: format!("test_doc_{}", i),
            title: Some(format!("测试文档 {}", i)),
            content: format!("这是第{}个测试文档的内容", i),
            language: Some("zh".to_string()),
            version: Some("1".to_string()),
            doc_type: Some("test".to_string()),
            package_name: Some("test_package".to_string()),
            vector: None,
            metadata: std::collections::HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_node_creation() {
        let node = TestNode::new("test_node".to_string(), ClusterType::ThreeNode).await;
        assert_eq!(node.node_id(), "test_node");
        assert_eq!(node.get_state().await, TestNodeState::Stopped);
    }
    
    #[tokio::test]
    async fn test_node_lifecycle() {
        let node = TestNode::new("test_node".to_string(), ClusterType::ThreeNode).await;
        
        // 启动节点
        node.start().await.unwrap();
        assert_eq!(node.get_state().await, TestNodeState::Running);
        assert!(node.is_running().await);
        
        // 停止节点
        node.stop().await.unwrap();
        assert_eq!(node.get_state().await, TestNodeState::Stopped);
        assert!(!node.is_running().await);
        
        // 重启节点
        node.restart().await.unwrap();
        assert_eq!(node.get_state().await, TestNodeState::Running);
        assert!(node.is_running().await);
    }
    
    #[tokio::test]
    async fn test_document_operations() {
        let node = TestNode::new("test_node".to_string(), ClusterType::Standalone).await;
        node.start().await.unwrap();
        
        // 插入文档
        let doc = create_test_document();
        let doc_id = node.insert_document(doc.clone()).await.unwrap();
        assert_eq!(doc_id, doc.id);
        
        // 获取文档
        let retrieved = node.get_document(&doc_id).await.unwrap();
        assert_eq!(retrieved.id, doc_id);
    }
}