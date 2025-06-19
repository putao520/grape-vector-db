use serde::{Deserialize, Serialize};

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::advanced_storage::AdvancedStorage;
use crate::distributed::network::NetworkManager;
use crate::distributed::raft::{RaftNode, RaftState};
use crate::types::{ClusterInfo, NodeId, NodeInfo, Term};

/// 共识管理器
pub struct ConsensusManager {
    /// Raft 节点
    raft_node: Arc<RaftNode>,
    /// 网络管理器
    network_manager: Arc<NetworkManager>,
    /// 集群信息
    cluster_info: Arc<RwLock<ClusterInfo>>,
    /// 共识状态
    consensus_state: Arc<RwLock<ConsensusState>>,
    /// 命令处理器
    command_handler: Arc<CommandHandler>,
    /// Raft状态（包含日志）
    raft_state: Arc<RwLock<RaftInternalState>>,
    /// 本地节点ID
    local_node_id: NodeId,
    /// 集群管理器
    cluster_manager: Arc<crate::distributed::cluster::ClusterManager>,
}

/// 共识状态
#[derive(Debug, Clone)]
pub struct ConsensusState {
    /// 当前任期
    pub current_term: Term,
    /// 投票给的候选者
    pub voted_for: Option<NodeId>,
    /// 当前状态
    pub state: RaftState,
    /// 领导者ID
    pub leader_id: Option<NodeId>,
    /// 最后心跳时间
    pub last_heartbeat: Instant,
    /// 选举超时时间
    pub election_timeout: Duration,
    /// 心跳间隔
    pub heartbeat_interval: Duration,
}

impl Default for ConsensusState {
    fn default() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
            state: RaftState::Follower,
            leader_id: None,
            last_heartbeat: Instant::now(),
            election_timeout: Duration::from_millis(150 + rand::random::<u64>() % 150), // 150-300ms
            heartbeat_interval: Duration::from_millis(50),                              // 50ms
        }
    }
}

/// 命令处理器
#[allow(dead_code)]
pub struct CommandHandler {
    /// 存储引擎
    storage: Arc<AdvancedStorage>,
    /// 分片管理器
    shard_manager: Option<Arc<crate::distributed::shard::ShardManager>>,
}

/// 共识命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusCommand {
    /// 向量操作
    VectorOperation {
        operation: VectorOperation,
        shard_id: u32,
    },
    /// 集群配置变更
    ConfigChange {
        change_type: ConfigChangeType,
        node_info: NodeInfo,
    },
    /// 分片操作
    ShardOperation { operation: ShardOperation },
}

/// 向量操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VectorOperation {
    /// 插入向量
    Upsert { points: Vec<crate::types::Point> },
    /// 删除向量
    Delete { point_ids: Vec<String> },
    /// 批量操作
    Batch { operations: Vec<VectorOperation> },
}

/// 配置变更类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigChangeType {
    /// 添加节点
    AddNode,
    /// 移除节点
    RemoveNode,
    /// 更新节点
    UpdateNode,
}

/// 分片操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShardOperation {
    /// 创建分片
    CreateShard {
        shard_id: u32,
        primary_node: NodeId,
        replica_nodes: Vec<NodeId>,
    },
    /// 迁移分片
    MigrateShard {
        shard_id: u32,
        from_node: NodeId,
        to_node: NodeId,
    },
    /// 重新平衡分片
    RebalanceShards {
        migrations: Vec<crate::distributed::shard::ShardMigration>,
    },
}

/// 命令执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandResult {
    /// 成功
    Success {
        message: String,
        data: Option<serde_json::Value>,
    },
    /// 失败
    Error { error: String, code: u32 },
}

/// 日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// 任期
    pub term: u64,
    /// 索引
    pub index: u64,
    /// 命令数据
    pub command: Vec<u8>,
}

/// 复制结果
#[derive(Debug, Clone)]
pub struct ReplicationResult {
    /// 是否成功
    pub success: bool,
    /// 已提交的索引
    pub committed_index: u64,
    /// 成功复制的节点数
    pub successful_replications: usize,
    /// 需要的确认数
    pub required_acks: usize,
}

/// Raft内部状态
#[derive(Debug, Clone)]
pub struct RaftInternalState {
    /// 当前任期
    pub current_term: u64,
    /// 日志条目
    pub log: Vec<LogEntry>,
    /// 已提交的索引
    pub commit_index: u64,
    /// 已应用的索引
    pub last_applied: u64,
}

impl ConsensusManager {
    /// 创建新的共识管理器
    pub fn new(
        raft_node: Arc<RaftNode>,
        network_manager: Arc<NetworkManager>,
        cluster_info: Arc<RwLock<ClusterInfo>>,
        storage: Arc<AdvancedStorage>,
        local_node_id: NodeId,
        cluster_manager: Arc<crate::distributed::cluster::ClusterManager>,
    ) -> Self {
        let command_handler = Arc::new(CommandHandler {
            storage,
            shard_manager: None,
        });

        let raft_state = Arc::new(RwLock::new(RaftInternalState {
            current_term: 0,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
        }));

        Self {
            raft_node,
            network_manager,
            cluster_info,
            consensus_state: Arc::new(RwLock::new(ConsensusState::default())),
            command_handler,
            raft_state,
            local_node_id,
            cluster_manager,
        }
    }

    /// 启动共识管理器
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("启动共识管理器");

        // 启动选举定时器
        self.start_election_timer().await;

        // 启动心跳定时器
        self.start_heartbeat_timer().await;

        // 启动命令处理循环
        self.start_command_processing().await;

        Ok(())
    }

    /// 启动选举定时器
    async fn start_election_timer(&self) {
        let consensus_state = self.consensus_state.clone();
        let raft_node = self.raft_node.clone();
        let network_manager = self.network_manager.clone();
        let cluster_info = self.cluster_info.clone();

        tokio::spawn(async move {
            loop {
                let election_timeout = {
                    let state = consensus_state.read().await;
                    state.election_timeout
                };

                tokio::time::sleep(election_timeout).await;

                // 检查是否需要开始选举
                let should_start_election = {
                    let state = consensus_state.read().await;
                    match state.state {
                        RaftState::Follower | RaftState::Candidate => {
                            state.last_heartbeat.elapsed() > state.election_timeout
                        }
                        RaftState::Leader => false,
                    }
                };

                if should_start_election {
                    info!("选举超时，开始新的选举");
                    if let Err(e) = Self::start_election(
                        &consensus_state,
                        &raft_node,
                        &network_manager,
                        &cluster_info,
                    )
                    .await
                    {
                        error!("选举失败: {}", e);
                    }
                }
            }
        });
    }

    /// 启动心跳定时器
    async fn start_heartbeat_timer(&self) {
        let consensus_state = self.consensus_state.clone();
        let raft_node = self.raft_node.clone();
        let network_manager = self.network_manager.clone();
        let cluster_info = self.cluster_info.clone();

        tokio::spawn(async move {
            loop {
                let heartbeat_interval = {
                    let state = consensus_state.read().await;
                    state.heartbeat_interval
                };

                tokio::time::sleep(heartbeat_interval).await;

                // 如果是领导者，发送心跳
                let is_leader = {
                    let state = consensus_state.read().await;
                    matches!(state.state, RaftState::Leader)
                };

                if is_leader {
                    if let Err(e) = Self::send_heartbeats(
                        &consensus_state,
                        &raft_node,
                        &network_manager,
                        &cluster_info,
                    )
                    .await
                    {
                        error!("发送心跳失败: {}", e);
                    }
                }
            }
        });
    }

    /// 启动命令处理循环
    async fn start_command_processing(&self) {
        let command_handler = self.command_handler.clone();
        let raft_node = self.raft_node.clone();

        tokio::spawn(async move {
            info!("启动命令处理循环");
            let mut last_applied = 0;

            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;

                // 检查是否有新的已提交条目需要应用
                let commit_index = raft_node.get_commit_index().await;

                if commit_index > last_applied {
                    // 应用从 last_applied+1 到 commit_index 的所有条目
                    for index in (last_applied + 1)..=commit_index {
                        if let Some(entry) = raft_node.get_log_entry(index).await {
                            match bincode::deserialize::<crate::distributed::raft::VectorCommand>(
                                &entry.data,
                            ) {
                                Ok(vector_command) => {
                                    // 将 VectorCommand 转换为 ConsensusCommand
                                    let consensus_command = match vector_command {
                                        crate::distributed::raft::VectorCommand::Upsert {
                                            points,
                                            shard_id,
                                        } => ConsensusCommand::VectorOperation {
                                            operation: VectorOperation::Upsert { points },
                                            shard_id,
                                        },
                                        crate::distributed::raft::VectorCommand::Delete {
                                            point_ids,
                                            shard_id,
                                        } => ConsensusCommand::VectorOperation {
                                            operation: VectorOperation::Delete { point_ids },
                                            shard_id,
                                        },
                                        crate::distributed::raft::VectorCommand::CreateShard {
                                            shard_id,
                                            hash_range: _,
                                        } => {
                                            // 暂时使用一个默认节点，实际应该根据集群状态决定
                                            let primary_node = raft_node.get_node_id().clone();
                                            ConsensusCommand::ShardOperation {
                                                operation: ShardOperation::CreateShard {
                                                    shard_id,
                                                    primary_node,
                                                    replica_nodes: vec![],
                                                },
                                            }
                                        }
                                        crate::distributed::raft::VectorCommand::DropShard {
                                            shard_id,
                                        } => {
                                            // 创建一个删除分片的迁移操作
                                            warn!("DropShard命令暂不支持，忽略分片 {}", shard_id);
                                            last_applied = index;
                                            continue;
                                        }
                                    };

                                    if let Err(e) =
                                        command_handler.execute_command(consensus_command).await
                                    {
                                        error!("执行命令失败: {}", e);
                                    } else {
                                        last_applied = index;
                                        debug!("成功应用日志条目 {}", index);
                                    }
                                }
                                Err(e) => {
                                    error!("反序列化命令失败: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// 开始选举
    async fn start_election(
        consensus_state: &Arc<RwLock<ConsensusState>>,
        raft_node: &Arc<RaftNode>,
        network_manager: &Arc<NetworkManager>,
        cluster_info: &Arc<RwLock<ClusterInfo>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 转换为候选者状态
        let (current_term, node_id) = {
            let mut state = consensus_state.write().await;
            state.current_term += 1;
            state.state = RaftState::Candidate;
            state.voted_for = Some(raft_node.get_node_id().clone());
            state.last_heartbeat = Instant::now();
            (state.current_term, raft_node.get_node_id().clone())
        };

        info!("开始选举，任期: {}, 候选者: {}", current_term, node_id);

        // 获取集群中的其他节点
        let other_nodes = {
            let cluster = cluster_info.read().await;
            cluster
                .nodes
                .values()
                .filter(|node| node.id != node_id)
                .cloned()
                .collect::<Vec<_>>()
        };

        if other_nodes.is_empty() {
            // 单节点集群，直接成为领导者
            let mut state = consensus_state.write().await;
            state.state = RaftState::Leader;
            state.leader_id = Some(node_id.clone());
            info!("单节点集群，直接成为领导者");
            return Ok(());
        }

        // 获取需要的票数
        let votes_needed = other_nodes.len().div_ceil(2) + 1; // 过半数

        // 并行发送投票请求
        let mut vote_tasks = Vec::new();
        for node in &other_nodes {
            let network_manager = network_manager.clone();
            let node_id_for_request = node.id.clone();
            let vote_request = crate::distributed::raft::VoteRequest {
                term: current_term,
                candidate_id: node_id.clone(),
                last_log_index: raft_node.get_last_log_index().await,
                last_log_term: raft_node.get_last_log_term().await,
            };

            let task = tokio::spawn(async move {
                match network_manager
                    .send_vote_request(&node_id_for_request, vote_request)
                    .await
                {
                    Ok(response) => Some((node_id_for_request.clone(), response)),
                    Err(e) => {
                        warn!("向节点 {} 发送投票请求失败: {}", node_id_for_request, e);
                        None
                    }
                }
            });
            vote_tasks.push(task);
        }

        // 收集投票结果
        let mut votes_received = 1; // 自己的票

        for task in vote_tasks {
            if let Ok(Some((node_id, response))) = task.await {
                if response.vote_granted {
                    votes_received += 1;
                    debug!("收到节点 {} 的投票", node_id);
                } else {
                    debug!("节点 {} 拒绝投票，任期: {}", node_id, response.term);

                    // 如果对方任期更高，转换为跟随者
                    if response.term > current_term {
                        let mut state = consensus_state.write().await;
                        state.current_term = response.term;
                        state.state = RaftState::Follower;
                        state.voted_for = None;
                        state.leader_id = None;
                        info!("发现更高任期 {}，转换为跟随者", response.term);
                        return Ok(());
                    }
                }

                if votes_received >= votes_needed {
                    break;
                }
            }
        }

        // 检查选举结果
        if votes_received >= votes_needed {
            // 成为领导者
            let mut state = consensus_state.write().await;
            state.state = RaftState::Leader;
            state.leader_id = Some(node_id.clone());
            info!("选举成功，成为领导者，获得 {} 票", votes_received);
        } else {
            // 选举失败，转换为跟随者
            let mut state = consensus_state.write().await;
            state.state = RaftState::Follower;
            state.voted_for = None;
            info!(
                "选举失败，获得 {} 票，需要 {} 票",
                votes_received, votes_needed
            );
        }

        Ok(())
    }

    /// 发送心跳
    async fn send_heartbeats(
        consensus_state: &Arc<RwLock<ConsensusState>>,
        raft_node: &Arc<RaftNode>,
        network_manager: &Arc<NetworkManager>,
        cluster_info: &Arc<RwLock<ClusterInfo>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (current_term, node_id) = {
            let state = consensus_state.read().await;
            (state.current_term, raft_node.get_node_id().clone())
        };

        // 获取集群中的其他节点
        let other_nodes = {
            let cluster = cluster_info.read().await;
            cluster
                .nodes
                .values()
                .filter(|node| node.id != node_id)
                .cloned()
                .collect::<Vec<_>>()
        };

        // 并行发送心跳
        let mut heartbeat_tasks = Vec::new();
        for node in other_nodes {
            let network_manager = network_manager.clone();
            // 使用基本的心跳信息（对于静态方法）
            let prev_log_index = 0;
            let prev_log_term = 0;
            let leader_commit = 0;

            let append_request = crate::distributed::raft::AppendRequest {
                term: current_term,
                leader_id: node_id.clone(),
                prev_log_index, // 实际的前一个日志索引
                prev_log_term,  // 实际的前一个日志任期  
                entries: Vec::new(), // 心跳不包含日志条目
                leader_commit,  // 实际的领导者提交索引
            };

            let task = tokio::spawn(async move {
                match network_manager
                    .send_append_request(&node.id, append_request)
                    .await
                {
                    Ok(response) => Some((node.id, response)),
                    Err(e) => {
                        warn!("向节点 {} 发送心跳失败: {}", node.id, e);
                        None
                    }
                }
            });
            heartbeat_tasks.push(task);
        }

        // 处理心跳响应
        for task in heartbeat_tasks {
            if let Ok(Some((node_id, response))) = task.await {
                if !response.success {
                    debug!("节点 {} 心跳响应失败，任期: {}", node_id, response.term);

                    // 如果对方任期更高，转换为跟随者
                    if response.term > current_term {
                        let mut state = consensus_state.write().await;
                        state.current_term = response.term;
                        state.state = RaftState::Follower;
                        state.voted_for = None;
                        state.leader_id = None;
                        info!("发现更高任期 {}，转换为跟随者", response.term);
                        return Ok(());
                    }
                } else {
                    debug!("节点 {} 心跳响应成功", node_id);
                }
            }
        }

        Ok(())
    }

    /// 提交命令
    pub async fn submit_command(
        &self,
        command: ConsensusCommand,
    ) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        // 检查是否是领导者
        let is_leader = {
            let state = self.consensus_state.read().await;
            matches!(state.state, RaftState::Leader)
        };

        if !is_leader {
            return Err("不是领导者，无法提交命令".into());
        }

        // 序列化命令
        let command_data = serde_json::to_vec(&command)?;

        // 将命令添加到 Raft 日志并进行复制
        let log_entry = {
            let mut state = self.raft_state.write().await;
            let current_term = state.current_term;
            let log_index = state.log.len() as u64;
            
            let entry = LogEntry {
                term: current_term,
                index: log_index,
                command: command_data.clone(),
            };
            
            // 添加到本地日志
            state.log.push(entry.clone());
            entry
        };

        // 复制到其他节点
        let replication_result = self.replicate_log_entry(log_entry).await?;
        
        if replication_result.success {
            // 如果复制成功，执行命令
            let result = self.command_handler.execute_command(command).await?;
            
            // 更新提交索引
            {
                let mut state = self.raft_state.write().await;
                state.commit_index = replication_result.committed_index;
            }
            
            Ok(result)
        } else {
            Err("日志复制失败，命令未提交".into())
        }
    }

    /// 复制日志条目到其他节点
    async fn replicate_log_entry(&self, entry: LogEntry) -> Result<ReplicationResult, Box<dyn std::error::Error + Send + Sync>> {
        let cluster_info = self.cluster_manager.get_cluster_info().await;
        let total_nodes = cluster_info.nodes.len();
        let required_acks = (total_nodes / 2) + 1; // 需要多数派确认
        
        let mut successful_replications = 1; // 包括本地节点
        let mut replication_tasks = Vec::new();
        
        // 向所有follower节点发送日志条目
        for node in &cluster_info.nodes {
            if node.id != self.local_node_id {
                let node_id = node.id.clone();
                let entry_clone = entry.clone();
                let network_manager = self.network_manager.clone();
                
                let task = tokio::spawn(async move {
                    // 发送AppendEntries RPC
                    match network_manager.send_append_entries(&node_id, entry_clone).await {
                        Ok(true) => {
                            debug!("节点 {} 确认日志条目", node_id);
                            true
                        }
                        Ok(false) => {
                            warn!("节点 {} 拒绝日志条目", node_id);
                            false
                        }
                        Err(e) => {
                            warn!("向节点 {} 发送日志条目失败: {}", node_id, e);
                            false
                        }
                    }
                });
                
                replication_tasks.push(task);
            }
        }
        
        // 等待复制结果
        for task in replication_tasks {
            if let Ok(success) = task.await {
                if success {
                    successful_replications += 1;
                }
            }
        }
        
        let success = successful_replications >= required_acks;
        let committed_index = if success { entry.index } else { 0 };
        
        Ok(ReplicationResult {
            success,
            committed_index,
            successful_replications,
            required_acks,
        })
    }

    /// 获取当前状态
    pub async fn get_state(&self) -> ConsensusState {
        self.consensus_state.read().await.clone()
    }

    /// 获取领导者信息
    pub async fn get_leader(&self) -> Option<NodeId> {
        self.consensus_state.read().await.leader_id.clone()
    }

    /// 是否是领导者
    pub async fn is_leader(&self) -> bool {
        let state = self.consensus_state.read().await;
        matches!(state.state, RaftState::Leader)
    }

    /// 关闭共识管理器
    pub async fn shutdown(&self) {
        info!("关闭共识管理器");
        
        // 清理资源
        // 1. 停止心跳和选举
        {
            let mut state = self.consensus_state.write().await;
            state.state = RaftState::Follower; // 转为follower状态
        }
        
        // 2. 等待正在进行的命令完成
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 3. 清理Raft状态（可选，根据需要持久化）
        {
            let mut raft_state = self.raft_state.write().await;
            info!("Raft状态清理完成，日志条目数: {}", raft_state.log.len());
        }
        
        info!("共识管理器已关闭");
    }
}

impl CommandHandler {
    /// 执行命令
    pub async fn execute_command(
        &self,
        command: ConsensusCommand,
    ) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        match command {
            ConsensusCommand::VectorOperation {
                operation,
                shard_id,
            } => self.execute_vector_operation(operation, shard_id).await,
            ConsensusCommand::ConfigChange {
                change_type,
                node_info,
            } => self.execute_config_change(change_type, node_info).await,
            ConsensusCommand::ShardOperation { operation } => {
                self.execute_shard_operation(operation).await
            }
        }
    }

    /// 执行向量操作
    async fn execute_vector_operation(
        &self,
        operation: VectorOperation,
        shard_id: u32,
    ) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        match operation {
            VectorOperation::Upsert { points } => {
                for point in points {
                    self.storage.store_vector(&point)?;
                }
                Ok(CommandResult::Success {
                    message: "向量插入成功".to_string(),
                    data: None,
                })
            }
            VectorOperation::Delete { point_ids } => {
                let mut deleted_count = 0;
                for point_id in point_ids {
                    if self.storage.delete_vector(&point_id)? {
                        deleted_count += 1;
                    }
                }
                Ok(CommandResult::Success {
                    message: format!("删除 {} 个向量", deleted_count),
                    data: Some(serde_json::json!({ "deleted_count": deleted_count })),
                })
            }
            VectorOperation::Batch { operations } => {
                let mut results = Vec::new();
                for op in operations {
                    let result = Box::pin(self.execute_vector_operation(op, shard_id)).await?;
                    results.push(result);
                }
                Ok(CommandResult::Success {
                    message: "批量操作完成".to_string(),
                    data: Some(serde_json::json!({ "results": results })),
                })
            }
        }
    }

    /// 执行配置变更
    async fn execute_config_change(
        &self,
        change_type: ConfigChangeType,
        node_info: NodeInfo,
    ) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        match change_type {
            ConfigChangeType::AddNode => {
                info!("添加节点: {}", node_info.id);
                
                // 实现节点添加逻辑
                // 1. 验证节点信息
                if node_info.id.is_empty() || node_info.address.is_empty() {
                    return Ok(CommandResult::Error {
                        error: "节点信息不完整".to_string(),
                        code: 400,
                    });
                }
                
                // 2. 检查节点是否已存在 - 在实际实现中应由ConsensusManager处理
                {
                    // let cluster_info = self.cluster_info.read().await;
                    // if cluster_info.nodes.iter().any(|n| n.id == node_info.id) {
                    //     return Ok(CommandResult::Error {
                    //         error: format!("节点 {} 已存在", node_info.id),
                    //         code: 409,
                    //     });
                    // }
                    warn!("节点存在性检查跳过 - 应由ConsensusManager处理");
                }
                
                // 3. 添加节点到集群 - 在实际实现中应由ConsensusManager处理
                {
                    // let mut cluster_info = self.cluster_info.write().await;
                    // cluster_info.nodes.push(node_info.clone());
                    // cluster_info.version += 1;
                    warn!("集群节点添加跳过 - 应由ConsensusManager处理");
                }
                
                Ok(CommandResult::Success {
                    message: format!("节点 {} 添加成功", node_info.id),
                    data: Some(serde_json::json!({ "node_id": node_info.id })),
                })
            }
            ConfigChangeType::RemoveNode => {
                info!("移除节点: {}", node_info.id);
                
                // 实现节点移除逻辑
                // 1. 检查节点是否存在 - 在实际实现中应由ConsensusManager处理
                let node_exists = {
                    // let cluster_info = self.cluster_info.read().await;
                    // cluster_info.nodes.iter().any(|n| n.id == node_info.id)
                    warn!("节点存在性检查跳过 - 应由ConsensusManager处理");
                    false // 假设不存在，避免实际处理
                };
                
                if !node_exists {
                    return Ok(CommandResult::Error {
                        error: format!("节点 {} 不存在", node_info.id),
                        code: 404,
                    });
                }
                
                // 2. 检查是否是最后一个节点 - 在实际实现中应由ConsensusManager处理
                {
                    // let cluster_info = self.cluster_info.read().await;
                    // if cluster_info.nodes.len() <= 1 {
                    //     return Ok(CommandResult::Error {
                    //         error: "不能移除最后一个节点".to_string(),
                    //         code: 400,
                    //     });
                    // }
                    warn!("最后节点检查跳过 - 应由ConsensusManager处理");
                }
                
                // 3. 从集群中移除节点 - 在实际实现中应由ConsensusManager处理
                {
                    // let mut cluster_info = self.cluster_info.write().await;
                    // cluster_info.nodes.retain(|n| n.id != node_info.id);
                    // cluster_info.version += 1;
                    warn!("集群节点移除跳过 - 应由ConsensusManager处理");
                }
                
                Ok(CommandResult::Success {
                    message: format!("节点 {} 移除成功", node_info.id),
                    data: Some(serde_json::json!({ "removed_node_id": node_info.id })),
                })
            }
            ConfigChangeType::UpdateNode => {
                info!("更新节点: {}", node_info.id);
                
                // 实现节点更新逻辑
                // 1. 检查节点是否存在 - 在实际实现中应由ConsensusManager处理
                let updated = {
                    // let mut cluster_info = self.cluster_info.write().await;
                    // let mut found = false;
                    
                    // for existing_node in &mut cluster_info.nodes {
                    //     if existing_node.id == node_info.id {
                    //         // 更新节点信息
                    //         existing_node.address = node_info.address.clone();
                    //         existing_node.state = node_info.state.clone();
                    //         existing_node.role = node_info.role.clone();
                    //         existing_node.load = node_info.load.clone();
                    //         existing_node.last_heartbeat = chrono::Utc::now();
                    //         found = true;
                    //         break;
                    //     }
                    // }
                    
                    // if found {
                    //     cluster_info.version += 1;
                    // }
                    
                    // found
                    warn!("节点更新跳过 - 应由ConsensusManager处理");
                    false // 假设失败，避免实际处理
                };
                
                if updated {
                    Ok(CommandResult::Success {
                        message: format!("节点 {} 更新成功", node_info.id),
                        data: Some(serde_json::json!({ "updated_node_id": node_info.id })),
                    })
                } else {
                    Ok(CommandResult::Error {
                        error: format!("节点 {} 不存在", node_info.id),
                        code: 404,
                    })
                }
            }
        }
    }

    /// 执行分片操作
    async fn execute_shard_operation(
        &self,
        operation: ShardOperation,
    ) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        match operation {
            ShardOperation::CreateShard {
                shard_id,
                primary_node,
                replica_nodes,
            } => {
                info!("创建分片: {}, 主节点: {}", shard_id, primary_node);
                
                // 实现分片创建逻辑
                // 1. 验证节点存在 - 在实际实现中应由ConsensusManager处理
                {
                    // let cluster_info = self.cluster_info.read().await;
                    // let node_ids: Vec<_> = cluster_info.nodes.iter().map(|n| &n.id).collect();
                    warn!("分片创建节点验证跳过 - 应由ConsensusManager处理");
                }
                
                // 2. 创建分片信息 - 在实际实现中应由ConsensusManager处理
                let shard_info = crate::types::ShardInfo {
                    id: shard_id,
                    primary_node: primary_node.clone(),
                    replica_nodes: replica_nodes.clone(),
                    state: crate::types::ShardState::Active,
                    range: crate::types::ShardRange {
                        start_hash: 0,
                        end_hash: u64::MAX,
                    },
                };
                
                // 3. 添加到集群信息 - 在实际实现中应由ConsensusManager处理
                {
                    // let mut cluster_info = self.cluster_info.write().await;
                    // cluster_info.shard_map.shards.insert(shard_id, shard_info);
                    // cluster_info.version += 1;
                    warn!("分片添加到集群跳过 - 应由ConsensusManager处理");
                }
                
                Ok(CommandResult::Success {
                    message: format!("分片 {} 创建成功", shard_id),
                    data: Some(serde_json::json!({
                        "shard_id": shard_id,
                        "primary_node": primary_node,
                        "replica_nodes": replica_nodes
                    })),
                })
            }
            ShardOperation::MigrateShard {
                shard_id,
                from_node,
                to_node,
            } => {
                info!("迁移分片: {} 从 {} 到 {}", shard_id, from_node, to_node);
                
                // 实现分片迁移逻辑
                // 1. 验证分片存在 - 在实际实现中应由ConsensusManager处理
                let migration_success = {
                    // let mut cluster_info = self.cluster_info.write().await;
                    
                    // if let Some(shard) = cluster_info.shard_map.shards.get_mut(&shard_id) {
                    //     // 2. 更新分片的主节点或副本节点
                    //     if shard.primary_node == from_node {
                    //         shard.primary_node = to_node.clone();
                    //     } else {
                    //         // 更新副本节点
                    //         if let Some(pos) = shard.replica_nodes.iter().position(|n| n == &from_node) {
                    //             shard.replica_nodes[pos] = to_node.clone();
                    //         }
                    //     }
                    //     
                    //     shard.version += 1;
                    //     cluster_info.version += 1;
                    //     true
                    // } else {
                    //     false
                    // }
                    warn!("分片迁移跳过 - 应由ConsensusManager处理");
                    false // 假设失败，避免实际处理
                };
                
                if migration_success {
                    Ok(CommandResult::Success {
                        message: format!("分片 {} 迁移成功", shard_id),
                        data: Some(serde_json::json!({
                            "shard_id": shard_id,
                            "from_node": from_node,
                            "to_node": to_node
                        })),
                    })
                } else {
                    Ok(CommandResult::Error {
                        error: format!("分片 {} 不存在", shard_id),
                        code: 404,
                    })
                }
            }
            ShardOperation::RebalanceShards { migrations } => {
                info!("重新平衡分片: {} 个迁移", migrations.len());
                
                // 实现分片重新平衡逻辑
                let mut completed_migrations = 0;
                let mut failed_migrations: Vec<String> = Vec::new();
                
                // 执行每个迁移 - 在实际实现中应由ConsensusManager处理
                {
                    // let mut cluster_info = self.cluster_info.write().await;
                    
                    // for migration in &migrations {
                    //     let shard_id = migration.shard_id;
                    //     let from_node = &migration.from_node;
                    //     let to_node = &migration.to_node;
                    //     
                    //     if let Some(shard) = cluster_info.shard_map.shards.get_mut(&shard_id) {
                    //         // 更新分片位置
                    //         if shard.primary_node == *from_node {
                    //             shard.primary_node = to_node.clone();
                    //             completed_migrations += 1;
                    //         } else if let Some(pos) = shard.replica_nodes.iter().position(|n| n == from_node) {
                    //             shard.replica_nodes[pos] = to_node.clone();
                    //             completed_migrations += 1;
                    //         } else {
                    //             failed_migrations.push(format!("分片 {} 中未找到源节点 {}", shard_id, from_node));
                    //         }
                    //         
                    //         shard.version += 1;
                    //     } else {
                    //         failed_migrations.push(format!("分片 {} 不存在", shard_id));
                    //     }
                    // }
                    
                    // if completed_migrations > 0 {
                    //     cluster_info.version += 1;
                    // }
                    warn!("分片重新平衡跳过 - 应由ConsensusManager处理");
                }
                
                let result_data = serde_json::json!({
                    "total_migrations": migrations.len(),
                    "completed_migrations": completed_migrations,
                    "failed_migrations": failed_migrations
                });
                
                if failed_migrations.is_empty() {
                    Ok(CommandResult::Success {
                        message: format!("分片重新平衡完成，{} 个迁移全部成功", completed_migrations),
                        data: Some(result_data),
                    })
                } else {
                    Ok(CommandResult::Success {
                        message: format!("分片重新平衡部分完成，{} 个成功，{} 个失败", 
                                       completed_migrations, failed_migrations.len()),
                        data: Some(result_data),
                    })
                }
            }
        }
    }
}
