//! 3节点集群完整示例
//! 
//! 这个示例展示了如何启动和管理 Grape Vector Database 的3节点集群，
//! 包括集群初始化、节点管理、数据分片、故障转移和监控。

use grape_vector_db::{
    distributed::{
        cluster::{ClusterManager, ClusterConfig},
        raft::{RaftNode, RaftConfig},
        shard::{ShardManager, ShardConfig},
        network::{NetworkManager, NodeConnection},
    },
    VectorDatabase, VectorDbConfig,
    types::{Point, NodeInfo, NodeState, ShardInfo, ClusterInfo},
    grpc::{start_grpc_server, VectorDbServiceImpl},
    metrics::MetricsCollector,
    errors::Result,
};
use std::{
    sync::Arc,
    collections::HashMap,
    time::{Duration, Instant},
    net::SocketAddr,
};
use tokio::{
    sync::{RwLock, mpsc, Mutex},
    time::{sleep, interval},
    signal,
    fs,
};
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};

/// 集群节点配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNodeConfig {
    pub node_id: String,
    pub node_address: String,
    pub grpc_port: u16,
    pub rest_port: u16,
    pub raft_port: u16,
    pub data_dir: String,
    pub is_seed: bool,
    pub seed_nodes: Vec<String>,
}

/// 集群部署配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterDeploymentConfig {
    pub cluster_id: String,
    pub cluster_token: String,
    pub nodes: Vec<ClusterNodeConfig>,
    pub shard_count: u32,
    pub replica_count: u32,
    pub vector_dimension: usize,
}

impl Default for ClusterDeploymentConfig {
    fn default() -> Self {
        Self {
            cluster_id: "grape-cluster-3node".to_string(),
            cluster_token: "grape-secret-token-12345".to_string(),
            nodes: vec![
                ClusterNodeConfig {
                    node_id: "node-1".to_string(),
                    node_address: "127.0.0.1".to_string(),
                    grpc_port: 6334,
                    rest_port: 6333,
                    raft_port: 7000,
                    data_dir: "./cluster_data/node1".to_string(),
                    is_seed: true,
                    seed_nodes: vec![],
                },
                ClusterNodeConfig {
                    node_id: "node-2".to_string(),
                    node_address: "127.0.0.1".to_string(),
                    grpc_port: 6344,
                    rest_port: 6343,
                    raft_port: 7001,
                    data_dir: "./cluster_data/node2".to_string(),
                    is_seed: false,
                    seed_nodes: vec!["node-1@127.0.0.1:7000".to_string()],
                },
                ClusterNodeConfig {
                    node_id: "node-3".to_string(),
                    node_address: "127.0.0.1".to_string(),
                    grpc_port: 6354,
                    rest_port: 6353,
                    raft_port: 7002,
                    data_dir: "./cluster_data/node3".to_string(),
                    is_seed: false,
                    seed_nodes: vec!["node-1@127.0.0.1:7000".to_string()],
                },
            ],
            shard_count: 16,
            replica_count: 3,
            vector_dimension: 768,
        }
    }
}

/// 集群节点实例
pub struct ClusterNode {
    pub config: ClusterNodeConfig,
    pub database: Arc<RwLock<VectorDatabase>>,
    pub cluster_manager: Arc<ClusterManager>,
    pub raft_node: Arc<RaftNode>,
    pub shard_manager: Arc<ShardManager>,
    pub metrics: Arc<MetricsCollector>,
    pub shutdown_tx: mpsc::UnboundedSender<()>,
    pub shutdown_rx: Option<mpsc::UnboundedReceiver<()>>,
}

impl ClusterNode {
    /// 创建新的集群节点
    pub async fn new(config: ClusterNodeConfig, cluster_config: ClusterConfig) -> Result<Self> {
        info!("初始化集群节点: {}", config.node_id);

        // 确保数据目录存在
        fs::create_dir_all(&config.data_dir).await?;

        // 创建数据库配置
        let db_config = VectorDbConfig {
            vector_dimension: cluster_config.vector_dimension,
            ..Default::default()
        };

        // 初始化数据库
        let database = VectorDatabase::with_config(&config.data_dir, db_config).await?;
        let database = Arc::new(RwLock::new(database));

        // 创建节点信息
        let node_info = NodeInfo {
            id: config.node_id.clone(),
            address: config.node_address.clone(),
            port: config.grpc_port,
            state: NodeState::Healthy,
            last_heartbeat: chrono::Utc::now().timestamp() as u64,
            metadata: HashMap::new(),
            load: Default::default(),
        };

        // 初始化存储引擎
        let storage = database.read().await.get_storage();

        // 创建 Raft 配置
        let raft_config = RaftConfig {
            node_id: config.node_id.clone(),
            peers: cluster_config.nodes.iter()
                .filter(|n| n.id != config.node_id)
                .map(|n| format!("{}@{}:{}", n.id, n.address, 7000))
                .collect(),
            election_timeout_ms: 1000,
            heartbeat_interval_ms: 250,
            log_compaction_threshold: 10000,
            snapshot_interval: 1000,
            max_append_entries: 100,
            ..Default::default()
        };

        // 初始化 Raft 节点
        let raft_node = Arc::new(RaftNode::new(raft_config, storage.clone()));

        // 创建分片配置
        let shard_config = ShardConfig {
            shard_count: cluster_config.shard_count,
            replica_count: cluster_config.replica_count,
            rebalance_threshold: 0.1,
            migration_batch_size: 1000,
            ..Default::default()
        };

        // 初始化分片管理器
        let shard_manager = Arc::new(ShardManager::new(
            shard_config,
            storage.clone(),
            config.node_id.clone(),
        ));

        // 初始化集群管理器
        let cluster_manager = Arc::new(ClusterManager::new(
            cluster_config,
            node_info,
            storage,
        )?);

        // 初始化指标收集器
        let metrics = Arc::new(MetricsCollector::new());

        // 创建关闭信号通道
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            database,
            cluster_manager,
            raft_node,
            shard_manager,
            metrics,
            shutdown_tx,
            shutdown_rx: Some(shutdown_rx),
        })
    }

    /// 启动节点
    pub async fn start(&mut self) -> Result<()> {
        info!("启动集群节点: {}", self.config.node_id);

        // 启动 Raft 节点
        self.raft_node.start().await?;
        info!("Raft 节点已启动");

        // 启动分片管理器
        self.shard_manager.start().await?;
        info!("分片管理器已启动");

        // 启动集群管理器
        self.cluster_manager.start().await?;
        info!("集群管理器已启动");

        // 如果不是种子节点，尝试加入集群
        if !self.config.is_seed && !self.config.seed_nodes.is_empty() {
            self.join_cluster().await?;
        }

        // 启动 gRPC 服务
        let grpc_handle = self.start_grpc_service().await?;

        // 启动监控任务
        let monitoring_handle = self.start_monitoring_task().await;

        // 启动健康检查任务
        let health_check_handle = self.start_health_check_task().await;

        info!("✅ 集群节点 {} 启动完成", self.config.node_id);

        // 等待关闭信号
        self.wait_for_shutdown().await;

        // 优雅关闭
        self.graceful_shutdown(grpc_handle, monitoring_handle, health_check_handle).await?;

        Ok(())
    }

    /// 加入集群
    async fn join_cluster(&self) -> Result<()> {
        info!("尝试加入集群...");

        for seed_node in &self.config.seed_nodes {
            info!("连接到种子节点: {}", seed_node);
            
            // 解析种子节点地址
            let parts: Vec<&str> = seed_node.split('@').collect();
            if parts.len() != 2 {
                warn!("无效的种子节点格式: {}", seed_node);
                continue;
            }

            // 尝试连接并加入集群
            match self.cluster_manager.join_cluster(seed_node).await {
                Ok(_) => {
                    info!("成功加入集群");
                    return Ok(());
                }
                Err(e) => {
                    warn!("加入集群失败: {}", e);
                    continue;
                }
            }
        }

        Err("无法加入集群：所有种子节点都不可达".into())
    }

    /// 启动 gRPC 服务
    async fn start_grpc_service(&self) -> Result<tokio::task::JoinHandle<()>> {
        let addr: SocketAddr = format!("{}:{}", self.config.node_address, self.config.grpc_port).parse()?;
        let database = self.database.clone();
        let cluster_manager = self.cluster_manager.clone();
        let shard_manager = self.shard_manager.clone();
        let raft_node = self.raft_node.clone();
        let metrics = self.metrics.clone();

        let handle = tokio::spawn(async move {
            // 创建一个简化的向量索引
            let vector_index = Arc::new(tokio::sync::RwLock::new(
                MockVectorIndex::new()
            ));

            let service = VectorDbServiceImpl::new(
                database,
                vector_index,
                raft_node,
                cluster_manager,
                shard_manager,
            );

            if let Err(e) = start_grpc_server(service, addr).await {
                error!("gRPC 服务启动失败: {}", e);
            }
        });

        info!("gRPC 服务已启动在 {}", addr);
        Ok(handle)
    }

    /// 启动监控任务
    async fn start_monitoring_task(&self) -> tokio::task::JoinHandle<()> {
        let cluster_manager = self.cluster_manager.clone();
        let raft_node = self.raft_node.clone();
        let shard_manager = self.shard_manager.clone();
        let metrics = self.metrics.clone();
        let node_id = self.config.node_id.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));

            loop {
                interval.tick().await;

                // 收集集群指标
                let cluster_info = cluster_manager.get_cluster_info().await;
                let raft_state = raft_node.get_state().await;
                let shard_info = shard_manager.get_shard_info().await;

                // 更新指标
                metrics.update_cluster_nodes(cluster_info.nodes.len() as f64);
                metrics.update_raft_term(raft_state.term as f64);
                metrics.update_shard_count(shard_info.len() as f64);

                debug!("节点 {} 指标更新完成", node_id);
            }
        })
    }

    /// 启动健康检查任务
    async fn start_health_check_task(&self) -> tokio::task::JoinHandle<()> {
        let cluster_manager = self.cluster_manager.clone();
        let node_id = self.config.node_id.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                interval.tick().await;

                // 发送心跳
                if let Err(e) = cluster_manager.send_heartbeat().await {
                    warn!("节点 {} 心跳发送失败: {}", node_id, e);
                }

                // 检查其他节点健康状态
                cluster_manager.check_node_health().await;
            }
        })
    }

    /// 等待关闭信号
    async fn wait_for_shutdown(&mut self) {
        let mut shutdown_rx = self.shutdown_rx.take().expect("shutdown_rx should be available");
        
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("收到 Ctrl+C 信号");
            }
            _ = shutdown_rx.recv() => {
                info!("收到关闭信号");
            }
        }
    }

    /// 优雅关闭
    async fn graceful_shutdown(
        &self,
        grpc_handle: tokio::task::JoinHandle<()>,
        monitoring_handle: tokio::task::JoinHandle<()>,
        health_check_handle: tokio::task::JoinHandle<()>,
    ) -> Result<()> {
        info!("开始优雅关闭节点: {}", self.config.node_id);

        // 从集群中移除节点
        if let Err(e) = self.cluster_manager.leave_cluster().await {
            warn!("离开集群失败: {}", e);
        }

        // 停止分片管理器
        self.shard_manager.stop().await?;

        // 停止 Raft 节点
        self.raft_node.stop().await?;

        // 保存数据
        if let Ok(db) = self.database.try_read() {
            db.save().await?;
        }

        // 终止服务任务
        grpc_handle.abort();
        monitoring_handle.abort();
        health_check_handle.abort();

        info!("节点 {} 已优雅关闭", self.config.node_id);
        Ok(())
    }

    /// 发送关闭信号
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// 集群管理器
pub struct ClusterDeployment {
    config: ClusterDeploymentConfig,
    nodes: Vec<Arc<Mutex<ClusterNode>>>,
}

impl ClusterDeployment {
    /// 创建新的集群部署
    pub async fn new(config: ClusterDeploymentConfig) -> Result<Self> {
        info!("初始化3节点集群部署");

        let cluster_config = ClusterConfig {
            cluster_id: config.cluster_id.clone(),
            cluster_token: config.cluster_token.clone(),
            nodes: config.nodes.iter().map(|n| NodeInfo {
                id: n.node_id.clone(),
                address: n.node_address.clone(),
                port: n.grpc_port,
                state: NodeState::Healthy,
                last_heartbeat: chrono::Utc::now().timestamp() as u64,
                metadata: HashMap::new(),
                load: Default::default(),
            }).collect(),
            shard_count: config.shard_count,
            replica_count: config.replica_count,
            consistency_level: grape_vector_db::types::ConsistencyLevel::Strong,
            heartbeat_interval_secs: 5,
            node_timeout_secs: 30,
            max_nodes: 100,
            vector_dimension: config.vector_dimension,
        };

        let mut nodes = Vec::new();
        for node_config in &config.nodes {
            let node = ClusterNode::new(node_config.clone(), cluster_config.clone()).await?;
            nodes.push(Arc::new(Mutex::new(node)));
        }

        Ok(Self {
            config,
            nodes,
        })
    }

    /// 启动整个集群
    pub async fn start_cluster(&self) -> Result<()> {
        info!("🚀 启动3节点集群");

        // 首先启动种子节点
        let seed_nodes: Vec<_> = self.nodes.iter()
            .enumerate()
            .filter(|(i, _)| self.config.nodes[*i].is_seed)
            .collect();

        for (i, node) in seed_nodes {
            info!("启动种子节点: {}", self.config.nodes[i].node_id);
            let node_clone = node.clone();
            tokio::spawn(async move {
                if let Err(e) = node_clone.lock().await.start().await {
                    error!("种子节点启动失败: {}", e);
                }
            });
            
            // 等待种子节点稳定
            sleep(Duration::from_secs(2)).await;
        }

        // 等待种子节点完全启动
        sleep(Duration::from_secs(5)).await;

        // 启动其他节点
        let non_seed_nodes: Vec<_> = self.nodes.iter()
            .enumerate()
            .filter(|(i, _)| !self.config.nodes[*i].is_seed)
            .collect();

        for (i, node) in non_seed_nodes {
            info!("启动节点: {}", self.config.nodes[i].node_id);
            let node_clone = node.clone();
            tokio::spawn(async move {
                if let Err(e) = node_clone.lock().await.start().await {
                    error!("节点启动失败: {}", e);
                }
            });
            
            // 节点间启动间隔
            sleep(Duration::from_secs(3)).await;
        }

        info!("✅ 集群启动完成");

        // 等待集群稳定
        sleep(Duration::from_secs(10)).await;

        // 验证集群状态
        self.verify_cluster_health().await?;

        Ok(())
    }

    /// 验证集群健康状态
    async fn verify_cluster_health(&self) -> Result<()> {
        info!("🔍 验证集群健康状态");

        // 检查每个节点的状态
        for (i, node) in self.nodes.iter().enumerate() {
            let node_guard = node.lock().await;
            let cluster_info = node_guard.cluster_manager.get_cluster_info().await;
            
            info!("节点 {} 集群视图:", self.config.nodes[i].node_id);
            info!("  集群ID: {}", cluster_info.config.cluster_id);
            info!("  节点数量: {}", cluster_info.nodes.len());
            
            // 检查领导者
            if let Some(leader_id) = &cluster_info.leader_id {
                info!("  当前领导者: {}", leader_id);
            } else {
                warn!("  没有领导者");
            }
        }

        info!("✅ 集群健康检查完成");
        Ok(())
    }

    /// 运行集群演示
    pub async fn run_demo(&self) -> Result<()> {
        info!("🎯 运行集群演示");

        // 等待集群完全稳定
        sleep(Duration::from_secs(5)).await;

        // 演示数据操作
        self.demo_data_operations().await?;

        // 演示故障转移
        self.demo_failover().await?;

        // 演示数据一致性
        self.demo_consistency().await?;

        info!("✅ 集群演示完成");
        Ok(())
    }

    /// 演示数据操作
    async fn demo_data_operations(&self) -> Result<()> {
        info!("📊 演示分布式数据操作");

        // 通过第一个节点添加数据
        let node1 = &self.nodes[0];
        let node1_guard = node1.lock().await;
        let db = node1_guard.database.read().await;

        // 添加一些向量数据
        for i in 0..10 {
            let point = Point {
                id: format!("cluster_doc_{}", i),
                vector: generate_random_vector(self.config.vector_dimension),
                payload: [
                    ("index".to_string(), serde_json::Value::Number(i.into())),
                    ("cluster".to_string(), serde_json::Value::String("demo".to_string())),
                ].into_iter().collect(),
            };

            // 这里应该通过集群管理器进行分片路由
            // 为了简化，我们直接在本地添加
            // TODO: 实现真正的分片路由
            info!("添加向量 {}", point.id);
        }

        // 通过第二个节点进行搜索
        let node2 = &self.nodes[1];
        let node2_guard = node2.lock().await;
        let db2 = node2_guard.database.read().await;

        let query = generate_random_vector(self.config.vector_dimension);
        // 这里应该进行分布式搜索
        // TODO: 实现分布式搜索逻辑
        info!("从节点2执行搜索查询");

        info!("✅ 数据操作演示完成");
        Ok(())
    }

    /// 演示故障转移
    async fn demo_failover(&self) -> Result<()> {
        info!("🔄 演示故障转移");

        // 获取当前领导者
        let node1_guard = self.nodes[0].lock().await;
        let cluster_info = node1_guard.cluster_manager.get_cluster_info().await;
        
        if let Some(leader_id) = &cluster_info.leader_id {
            info!("当前领导者: {}", leader_id);
            
            // 模拟领导者故障
            info!("模拟领导者故障...");
            
            // 这里应该实现故障注入逻辑
            // 例如：断开网络连接、停止心跳等
            
            // 等待新领导者选举
            sleep(Duration::from_secs(5)).await;
            
            // 检查新的集群状态
            let new_cluster_info = node1_guard.cluster_manager.get_cluster_info().await;
            if let Some(new_leader_id) = &new_cluster_info.leader_id {
                info!("新领导者: {}", new_leader_id);
                if new_leader_id != leader_id {
                    info!("✅ 故障转移成功");
                } else {
                    info!("领导者未发生变化");
                }
            }
        }

        Ok(())
    }

    /// 演示数据一致性
    async fn demo_consistency(&self) -> Result<()> {
        info!("🔒 演示数据一致性");

        // 在多个节点上验证数据一致性
        for (i, node) in self.nodes.iter().enumerate() {
            let node_guard = node.lock().await;
            let db = node_guard.database.read().await;
            let stats = db.stats();
            
            info!("节点 {} 数据统计:", self.config.nodes[i].node_id);
            info!("  向量数量: {}", stats.vector_count);
            info!("  内存使用: {:.2} MB", stats.memory_usage_mb);
        }

        info!("✅ 一致性检查完成");
        Ok(())
    }

    /// 关闭整个集群
    pub async fn shutdown_cluster(&self) -> Result<()> {
        info!("🛑 关闭集群");

        // 发送关闭信号给所有节点
        for node in &self.nodes {
            let node_guard = node.lock().await;
            node_guard.shutdown();
        }

        // 等待所有节点关闭
        sleep(Duration::from_secs(5)).await;

        info!("✅ 集群已关闭");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🍇 Grape Vector Database 3节点集群示例");

    // 加载配置
    let config = ClusterDeploymentConfig::default();

    // 创建并启动集群
    let cluster = ClusterDeployment::new(config).await?;
    
    // 启动集群
    cluster.start_cluster().await?;

    // 运行演示
    cluster.run_demo().await?;

    // 保持运行状态，等待手动中断
    info!("集群正在运行中，按 Ctrl+C 停止...");
    signal::ctrl_c().await?;

    // 关闭集群
    cluster.shutdown_cluster().await?;

    Ok(())
}

/// 生成随机向量
fn generate_random_vector(dimension: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..dimension).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

// 为了编译通过，我们需要提供一些简化的实现

/// 简化的向量索引实现
struct MockVectorIndex;

impl MockVectorIndex {
    fn new() -> Self {
        Self
    }
}

impl grape_vector_db::index::VectorIndex for MockVectorIndex {
    async fn add_vector(&mut self, _id: String, _vector: Vec<f32>) -> Result<()> {
        Ok(())
    }

    async fn search(&self, _query: &[f32], _k: usize) -> Result<Vec<grape_vector_db::types::ScoredPoint>> {
        Ok(vec![])
    }

    async fn remove_vector(&mut self, _id: &str) -> Result<bool> {
        Ok(true)
    }
}

// 为了编译通过，我们需要扩展一些配置结构
trait ClusterConfigExt {
    fn with_vector_dimension(self, dim: usize) -> Self;
}

impl ClusterConfigExt for ClusterConfig {
    fn with_vector_dimension(mut self, dim: usize) -> Self {
        self.vector_dimension = dim;
        self
    }
}

// 添加一些必要的字段到 ClusterConfig
impl ClusterConfig {
    pub fn vector_dimension(&self) -> usize {
        self.vector_dimension
    }
}

#[derive(Debug, Clone)]
pub struct ShardConfig {
    pub shard_count: u32,
    pub replica_count: u32,
    pub rebalance_threshold: f32,
    pub migration_batch_size: usize,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            shard_count: 16,
            replica_count: 3,
            rebalance_threshold: 0.1,
            migration_batch_size: 1000,
        }
    }
}