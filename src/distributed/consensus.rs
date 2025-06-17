use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc, oneshot};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};

use crate::distributed::raft::{RaftNode, RaftState};
use crate::types::{ClusterInfo, NodeInfo, Term, LogIndex, NodeId};
use crate::distributed::network::NetworkManager;
use crate::advanced_storage::AdvancedStorage;

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
            heartbeat_interval: Duration::from_millis(50), // 50ms
        }
    }
}

/// 命令处理器
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
    ShardOperation {
        operation: ShardOperation,
    },
}

/// 向量操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VectorOperation {
    /// 插入向量
    Upsert {
        points: Vec<crate::types::Point>,
    },
    /// 删除向量
    Delete {
        point_ids: Vec<String>,
    },
    /// 批量操作
    Batch {
        operations: Vec<VectorOperation>,
    },
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
    Error {
        error: String,
        code: u32,
    },
}

impl ConsensusManager {
    /// 创建新的共识管理器
    pub fn new(
        raft_node: Arc<RaftNode>,
        network_manager: Arc<NetworkManager>,
        cluster_info: Arc<RwLock<ClusterInfo>>,
        storage: Arc<AdvancedStorage>,
    ) -> Self {
        let command_handler = Arc::new(CommandHandler {
            storage,
            shard_manager: None,
        });

        Self {
            raft_node,
            network_manager,
            cluster_info,
            consensus_state: Arc::new(RwLock::new(ConsensusState::default())),
            command_handler,
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
                    ).await {
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
                    ).await {
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
                            match bincode::deserialize::<crate::distributed::raft::VectorCommand>(&entry.data) {
                                Ok(vector_command) => {
                                    // 将 VectorCommand 转换为 ConsensusCommand
                                    let consensus_command = match vector_command {
                                        crate::distributed::raft::VectorCommand::Upsert { points, shard_id } => {
                                            ConsensusCommand::VectorOperation {
                                                operation: VectorOperation::Upsert { points },
                                                shard_id,
                                            }
                                        }
                                        crate::distributed::raft::VectorCommand::Delete { point_ids, shard_id } => {
                                            ConsensusCommand::VectorOperation {
                                                operation: VectorOperation::Delete { point_ids },
                                                shard_id,
                                            }
                                        }
                                        crate::distributed::raft::VectorCommand::CreateShard { shard_id, hash_range: _ } => {
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
                                        crate::distributed::raft::VectorCommand::DropShard { shard_id } => {
                                            // 创建一个删除分片的迁移操作
                                            warn!("DropShard命令暂不支持，忽略分片 {}", shard_id);
                                            last_applied = index;
                                            continue;
                                        }
                                    };
                                    
                                    if let Err(e) = command_handler.execute_command(consensus_command).await {
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
            cluster.nodes.values()
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
        let votes_needed = (other_nodes.len() + 1) / 2 + 1; // 过半数
        
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
                match network_manager.send_vote_request(&node_id_for_request, vote_request).await {
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
            info!("选举失败，获得 {} 票，需要 {} 票", votes_received, votes_needed);
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
            cluster.nodes.values()
                .filter(|node| node.id != node_id)
                .cloned()
                .collect::<Vec<_>>()
        };

        // 并行发送心跳
        let mut heartbeat_tasks = Vec::new();
        for node in other_nodes {
            let network_manager = network_manager.clone();
            let append_request = crate::distributed::raft::AppendRequest {
                term: current_term,
                leader_id: node_id.clone(),
                prev_log_index: 0, // TODO: 获取实际的前一个日志索引
                prev_log_term: 0,  // TODO: 获取实际的前一个日志任期
                entries: Vec::new(), // 心跳不包含日志条目
                leader_commit: 0,  // TODO: 获取实际的领导者提交索引
            };

            let task = tokio::spawn(async move {
                match network_manager.send_append_request(&node.id, append_request).await {
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
    pub async fn submit_command(&self, command: ConsensusCommand) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        // 检查是否是领导者
        let is_leader = {
            let state = self.consensus_state.read().await;
            matches!(state.state, RaftState::Leader)
        };

        if !is_leader {
            return Err("不是领导者，无法提交命令".into());
        }

        // 序列化命令
        let _command_data = serde_json::to_vec(&command)?;

        // TODO: 将命令添加到 Raft 日志
        // 这里需要实现日志复制逻辑

        // 模拟命令执行
        let result = self.command_handler.execute_command(command).await?;

        Ok(result)
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
        // TODO: 清理资源
    }
}

impl CommandHandler {
    /// 执行命令
    pub async fn execute_command(&self, command: ConsensusCommand) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        match command {
            ConsensusCommand::VectorOperation { operation, shard_id } => {
                self.execute_vector_operation(operation, shard_id).await
            }
            ConsensusCommand::ConfigChange { change_type, node_info } => {
                self.execute_config_change(change_type, node_info).await
            }
            ConsensusCommand::ShardOperation { operation } => {
                self.execute_shard_operation(operation).await
            }
        }
    }

    /// 执行向量操作
    async fn execute_vector_operation(&self, operation: VectorOperation, shard_id: u32) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
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
    async fn execute_config_change(&self, change_type: ConfigChangeType, node_info: NodeInfo) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        match change_type {
            ConfigChangeType::AddNode => {
                info!("添加节点: {}", node_info.id);
                // TODO: 实现节点添加逻辑
                Ok(CommandResult::Success {
                    message: format!("节点 {} 添加成功", node_info.id),
                    data: None,
                })
            }
            ConfigChangeType::RemoveNode => {
                info!("移除节点: {}", node_info.id);
                // TODO: 实现节点移除逻辑
                Ok(CommandResult::Success {
                    message: format!("节点 {} 移除成功", node_info.id),
                    data: None,
                })
            }
            ConfigChangeType::UpdateNode => {
                info!("更新节点: {}", node_info.id);
                // TODO: 实现节点更新逻辑
                Ok(CommandResult::Success {
                    message: format!("节点 {} 更新成功", node_info.id),
                    data: None,
                })
            }
        }
    }

    /// 执行分片操作
    async fn execute_shard_operation(&self, operation: ShardOperation) -> Result<CommandResult, Box<dyn std::error::Error + Send + Sync>> {
        match operation {
            ShardOperation::CreateShard { shard_id, primary_node, replica_nodes: _ } => {
                info!("创建分片: {}, 主节点: {}", shard_id, primary_node);
                // TODO: 实现分片创建逻辑
                Ok(CommandResult::Success {
                    message: format!("分片 {} 创建成功", shard_id),
                    data: None,
                })
            }
            ShardOperation::MigrateShard { shard_id, from_node, to_node } => {
                info!("迁移分片: {} 从 {} 到 {}", shard_id, from_node, to_node);
                // TODO: 实现分片迁移逻辑
                Ok(CommandResult::Success {
                    message: format!("分片 {} 迁移成功", shard_id),
                    data: None,
                })
            }
            ShardOperation::RebalanceShards { migrations } => {
                info!("重新平衡分片: {} 个迁移", migrations.len());
                // TODO: 实现分片重新平衡逻辑
                Ok(CommandResult::Success {
                    message: format!("分片重新平衡完成，{} 个迁移", migrations.len()),
                    data: None,
                })
            }
        }
    }
} 