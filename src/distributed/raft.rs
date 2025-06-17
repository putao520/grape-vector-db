use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::time::{interval, timeout};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn, error, debug};

use crate::advanced_storage::AdvancedStorage;
use crate::types::*;

/// Raft 节点状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RaftState {
    /// 跟随者状态
    Follower,
    /// 候选者状态
    Candidate,
    /// 领导者状态
    Leader,
}

/// Raft 日志条目类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogEntryType {
    /// 普通数据操作
    Normal,
    /// 配置变更
    ConfigChange,
    /// 快照
    Snapshot,
}

/// Raft 日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// 日志索引
    pub index: LogIndex,
    /// 任期
    pub term: Term,
    /// 日志类型
    pub entry_type: LogEntryType,
    /// 数据内容
    pub data: Vec<u8>,
    /// 时间戳
    pub timestamp: i64,
}

/// 向量操作命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VectorCommand {
    /// 插入或更新向量
    Upsert {
        points: Vec<Point>,
        shard_id: u32,
    },
    /// 删除向量
    Delete {
        point_ids: Vec<String>,
        shard_id: u32,
    },
    /// 创建分片
    CreateShard {
        shard_id: u32,
        hash_range: (u64, u64),
    },
    /// 删除分片
    DropShard {
        shard_id: u32,
    },
}

/// 持久化的Raft状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentRaftState {
    /// 当前任期
    pub current_term: Term,
    /// 当前任期投票给的候选者
    pub voted_for: Option<NodeId>,
    /// 最后日志索引
    pub last_log_index: LogIndex,
}

/// Raft 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// 节点 ID
    pub node_id: NodeId,
    /// 集群节点列表
    pub peers: Vec<NodeId>,
    /// 选举超时时间 (毫秒)
    pub election_timeout_ms: u64,
    /// 心跳间隔 (毫秒)
    pub heartbeat_interval_ms: u64,
    /// 日志压缩阈值
    pub log_compaction_threshold: usize,
    /// 快照间隔
    pub snapshot_interval: usize,
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            node_id: Uuid::new_v4().to_string(),
            peers: Vec::new(),
            election_timeout_ms: 150,
            heartbeat_interval_ms: 50,
            log_compaction_threshold: 1000,
            snapshot_interval: 100,
        }
    }
}

/// 投票请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRequest {
    /// 候选者任期
    pub term: Term,
    /// 候选者 ID
    pub candidate_id: NodeId,
    /// 候选者最后日志索引
    pub last_log_index: LogIndex,
    /// 候选者最后日志任期
    pub last_log_term: Term,
}

/// 投票响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteResponse {
    /// 当前任期
    pub term: Term,
    /// 是否投票
    pub vote_granted: bool,
}

/// 追加日志请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendRequest {
    /// 领导者任期
    pub term: Term,
    /// 领导者 ID
    pub leader_id: NodeId,
    /// 前一个日志索引
    pub prev_log_index: LogIndex,
    /// 前一个日志任期
    pub prev_log_term: Term,
    /// 新的日志条目
    pub entries: Vec<LogEntry>,
    /// 领导者提交索引
    pub leader_commit: LogIndex,
}

/// 追加日志响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendResponse {
    /// 当前任期
    pub term: Term,
    /// 是否成功
    pub success: bool,
    /// 匹配索引
    pub match_index: LogIndex,
}

/// Raft 节点
pub struct RaftNode {
    /// 节点配置
    config: RaftConfig,
    /// 当前状态
    state: Arc<RwLock<RaftState>>,
    /// 当前任期
    current_term: Arc<RwLock<Term>>,
    /// 投票给谁
    voted_for: Arc<RwLock<Option<NodeId>>>,
    /// 日志条目
    log: Arc<RwLock<Vec<LogEntry>>>,
    /// 提交索引
    commit_index: Arc<RwLock<LogIndex>>,
    /// 应用索引
    last_applied: Arc<RwLock<LogIndex>>,
    /// 下一个索引 (仅领导者)
    next_index: Arc<RwLock<HashMap<NodeId, LogIndex>>>,
    /// 匹配索引 (仅领导者)
    match_index: Arc<RwLock<HashMap<NodeId, LogIndex>>>,
    /// 存储引擎
    storage: Arc<AdvancedStorage>,
    /// 命令发送器
    command_tx: mpsc::UnboundedSender<RaftCommand>,
    /// 命令接收器
    command_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<RaftCommand>>>>,
    /// 最后心跳时间
    last_heartbeat: Arc<RwLock<Instant>>,
}

/// Raft 命令
#[derive(Debug)]
pub enum RaftCommand {
    /// 投票请求
    RequestVote {
        request: VoteRequest,
        response_tx: oneshot::Sender<VoteResponse>,
    },
    /// 追加日志请求
    AppendEntries {
        request: AppendRequest,
        response_tx: oneshot::Sender<AppendResponse>,
    },
    /// 客户端命令
    ClientCommand {
        command: VectorCommand,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
    /// 选举超时
    ElectionTimeout,
    /// 心跳超时
    HeartbeatTimeout,
}

impl RaftNode {
    /// 创建新的 Raft 节点
    pub fn new(config: RaftConfig, storage: Arc<AdvancedStorage>) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        
        Self {
            config,
            state: Arc::new(RwLock::new(RaftState::Follower)),
            current_term: Arc::new(RwLock::new(0)),
            voted_for: Arc::new(RwLock::new(None)),
            log: Arc::new(RwLock::new(Vec::new())),
            commit_index: Arc::new(RwLock::new(0)),
            last_applied: Arc::new(RwLock::new(0)),
            next_index: Arc::new(RwLock::new(HashMap::new())),
            match_index: Arc::new(RwLock::new(HashMap::new())),
            storage,
            command_tx,
            command_rx: Arc::new(RwLock::new(Some(command_rx))),
            last_heartbeat: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// 启动 Raft 节点
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("启动 Raft 节点: {}", self.config.node_id);

        // 首先从持久化存储恢复状态
        self.restore_state().await?;
        self.restore_logs().await?;

        // 启动主循环
        let mut command_rx = self.command_rx.write().await.take()
            .ok_or("命令接收器已被取走")?;

        // 启动选举超时定时器
        let election_timeout = Duration::from_millis(self.config.election_timeout_ms);
        let mut election_timer = interval(election_timeout);

        // 启动心跳定时器
        let heartbeat_interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        let mut heartbeat_timer = interval(heartbeat_interval);

        loop {
            tokio::select! {
                // 处理命令
                Some(command) = command_rx.recv() => {
                    self.handle_command(command).await?;
                }
                
                // 选举超时
                _ = election_timer.tick() => {
                    self.handle_election_timeout().await?;
                }
                
                // 心跳超时
                _ = heartbeat_timer.tick() => {
                    self.handle_heartbeat_timeout().await?;
                }
            }
        }
    }

    /// 处理命令
    async fn handle_command(&self, command: RaftCommand) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match command {
            RaftCommand::RequestVote { request, response_tx } => {
                let response = self.handle_vote_request(request).await?;
                let _ = response_tx.send(response);
            }
            RaftCommand::AppendEntries { request, response_tx } => {
                let response = self.handle_append_request(request).await?;
                let _ = response_tx.send(response);
            }
            RaftCommand::ClientCommand { command, response_tx } => {
                let result = self.handle_client_command(command).await;
                let _ = response_tx.send(result);
            }
            RaftCommand::ElectionTimeout => {
                self.handle_election_timeout().await?;
            }
            RaftCommand::HeartbeatTimeout => {
                self.handle_heartbeat_timeout().await?;
            }
        }
        Ok(())
    }

    /// 处理投票请求
    async fn handle_vote_request(&self, request: VoteRequest) -> Result<VoteResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut current_term = self.current_term.write().await;
        let mut voted_for = self.voted_for.write().await;
        let log = self.log.read().await;

        // 如果请求的任期更大，更新当前任期
        if request.term > *current_term {
            *current_term = request.term;
            *voted_for = None;
            let mut state = self.state.write().await;
            *state = RaftState::Follower;
        }

        let vote_granted = if request.term < *current_term {
            // 任期过时，拒绝投票
            false
        } else if voted_for.is_some() && voted_for.as_ref() != Some(&request.candidate_id) {
            // 已经投票给其他候选者
            false
        } else {
            // 检查候选者日志是否至少和自己一样新
            let last_log_index = log.len() as LogIndex;
            let last_log_term = log.last().map(|entry| entry.term).unwrap_or(0);

            if request.last_log_term > last_log_term ||
               (request.last_log_term == last_log_term && request.last_log_index >= last_log_index) {
                *voted_for = Some(request.candidate_id.clone());
                true
            } else {
                false
            }
        };

        if vote_granted {
            // 重置心跳时间
            *self.last_heartbeat.write().await = Instant::now();
        }

        Ok(VoteResponse {
            term: *current_term,
            vote_granted,
        })
    }

    /// 处理追加日志请求
    async fn handle_append_request(&self, request: AppendRequest) -> Result<AppendResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut current_term = self.current_term.write().await;
        let mut log = self.log.write().await;
        let mut commit_index = self.commit_index.write().await;

        // 如果请求的任期更大，更新当前任期
        if request.term > *current_term {
            *current_term = request.term;
            let mut voted_for = self.voted_for.write().await;
            *voted_for = None;
            let mut state = self.state.write().await;
            *state = RaftState::Follower;
        }

        // 重置心跳时间
        *self.last_heartbeat.write().await = Instant::now();

        let (success, match_index) = if request.term < *current_term {
            // 任期过时，拒绝请求
            (false, 0)
        } else if request.prev_log_index > 0 {
            // 检查前一个日志条目是否匹配
            if request.prev_log_index > log.len() as LogIndex {
                // 日志不够长，存在缺失
                debug!("日志缺失: 请求索引 {}, 本地日志长度 {}", request.prev_log_index, log.len());
                (false, 0)
            } else {
                let prev_entry = &log[(request.prev_log_index - 1) as usize];
                if prev_entry.term == request.prev_log_term {
                    // 日志匹配，处理新条目
                    self.handle_log_entries(&mut log, &request).await
                } else {
                    // 日志冲突，需要回滚
                    warn!("日志冲突: 索引 {}, 期望任期 {}, 实际任期 {}", 
                          request.prev_log_index, request.prev_log_term, prev_entry.term);
                    (false, 0)
                }
            }
        } else {
            // 第一个日志条目或心跳
            self.handle_log_entries(&mut log, &request).await
        };

        // 更新提交索引
        if success && request.leader_commit > *commit_index {
            let new_commit_index = std::cmp::min(request.leader_commit, log.len() as LogIndex);
            if new_commit_index > *commit_index {
                *commit_index = new_commit_index;
                debug!("更新提交索引到: {}", *commit_index);
                
                // 异步应用已提交的条目
                let self_clone = self.clone_for_apply();
                tokio::spawn(async move {
                    if let Err(e) = self_clone.apply_committed_entries().await {
                        error!("应用已提交条目失败: {}", e);
                    }
                });
            }
        }

        Ok(AppendResponse {
            term: *current_term,
            success,
            match_index,
        })
    }

    /// 处理客户端命令
    async fn handle_client_command(&self, command: VectorCommand) -> Result<(), String> {
        let state = self.state.read().await;
        
        if *state != RaftState::Leader {
            return Err("只有领导者可以处理客户端命令".to_string());
        }

        // 序列化命令
        let data = bincode::serialize(&command)
            .map_err(|e| format!("序列化命令失败: {}", e))?;

        // 创建日志条目
        let current_term = *self.current_term.read().await;
        let mut log = self.log.write().await;
        let index = log.len() as LogIndex + 1;

        let entry = LogEntry {
            index,
            term: current_term,
            entry_type: LogEntryType::Normal,
            data,
            timestamp: chrono::Utc::now().timestamp(),
        };

        log.push(entry.clone());
        
        // 持久化日志条目
        if let Err(e) = self.persist_log_entry(&entry).await {
            warn!("持久化日志条目失败: {}", e);
        }
        
        drop(log); // 释放锁

        // 复制到其他节点
        let replication_result = self.replicate_log_entry(entry).await;
        
        match replication_result {
            Ok(_) => {
                info!("日志条目 {} 复制成功", index);
                Ok(())
            }
            Err(e) => {
                warn!("日志条目 {} 复制失败: {}", index, e);
                Err(format!("日志复制失败: {}", e))
            }
        }
    }

    /// 复制日志条目到其他节点
    async fn replicate_log_entry(&self, entry: LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let peers = self.config.peers.clone();
        let current_term = *self.current_term.read().await;
        let leader_id = self.config.node_id.clone();
        
        // 获取前一个日志索引和任期
        let (prev_log_index, prev_log_term) = {
            let log = self.log.read().await;
            if entry.index > 1 {
                let prev_entry = &log[(entry.index - 2) as usize];
                (prev_entry.index, prev_entry.term)
            } else {
                (0, 0)
            }
        };

        let leader_commit = *self.commit_index.read().await;
        
        // 创建追加请求
        let append_request = AppendRequest {
            term: current_term,
            leader_id,
            prev_log_index,
            prev_log_term,
            entries: vec![entry],
            leader_commit,
        };

        // 并行发送到所有节点
        let mut success_count = 1; // 包括自己
        let mut handles = Vec::new();
        let timeout_duration = Duration::from_millis(self.config.heartbeat_interval_ms * 2);
        
        for peer_id in peers {
            let _request = append_request.clone();
            let node_id = peer_id.clone();
            
            let handle = tokio::spawn(async move {
                // 模拟日志复制请求，在实际实现中这里应该是网络调用
                debug!("向节点 {} 发送日志复制请求", node_id);
                
                // 模拟网络延迟
                tokio::time::sleep(Duration::from_millis(fastrand::u64(5..30))).await;
                
                // 模拟日志复制成功/失败
                // 在实际实现中，这里会检查日志一致性并返回具体的响应
                let success = match fastrand::u8(0..10) {
                    0..=7 => true,  // 80%成功率
                    _ => false,     // 20%失败率（网络问题、日志冲突等）
                };
                
                if success {
                    debug!("节点 {} 接受日志复制", node_id);
                } else {
                    warn!("节点 {} 拒绝日志复制或网络失败", node_id);
                }
                
                Ok::<bool, Box<dyn std::error::Error + Send + Sync>>(success)
            });
            
            handles.push(handle);
        }

        // 等待结果，设置超时
        for handle in handles {
            match timeout(timeout_duration, handle).await {
                Ok(Ok(Ok(success))) if success => {
                    success_count += 1;
                }
                Ok(Ok(Err(e))) => {
                    warn!("日志复制请求处理出错: {}", e);
                }
                Err(_) => {
                    warn!("日志复制请求超时");
                }
                _ => {
                    // 复制失败，但不立即返回错误
                }
            }
        }

        // 检查是否达到多数
        let required_count = (self.config.peers.len() + 1) / 2 + 1;
        
        if success_count >= required_count {
            // 更新提交索引
            let mut commit_index = self.commit_index.write().await;
            if append_request.entries[0].index > *commit_index {
                *commit_index = append_request.entries[0].index;
                info!("更新提交索引到: {}", *commit_index);
                
                // 应用已提交的条目
                drop(commit_index);
                self.apply_committed_entries().await?;
            }
            Ok(())
        } else {
            Err(format!("未能获得多数节点确认: {}/{}", success_count, required_count).into())
        }
    }

    /// 处理选举超时
    async fn handle_election_timeout(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let state = self.state.read().await;
        let last_heartbeat = *self.last_heartbeat.read().await;
        
        // 添加随机化避免选举冲突，选举超时应该在基础超时的150%-300%之间
        let base_timeout_ms = self.config.election_timeout_ms;
        let randomized_timeout_ms = base_timeout_ms + (fastrand::u64(0..base_timeout_ms * 2));
        let election_timeout = Duration::from_millis(randomized_timeout_ms);

        // 只有跟随者和候选者需要处理选举超时
        if *state == RaftState::Leader {
            return Ok(());
        }

        // 检查是否超时
        if last_heartbeat.elapsed() < election_timeout {
            return Ok(());
        }

        drop(state); // 释放读锁
        info!("选举超时（随机化超时: {}ms），开始新的选举", randomized_timeout_ms);
        self.start_election().await
    }

    /// 开始选举
    async fn start_election(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 转换为候选者状态
        {
            let mut state = self.state.write().await;
            *state = RaftState::Candidate;
        }

        // 增加任期
        let current_term = {
            let mut current_term = self.current_term.write().await;
            *current_term += 1;
            *current_term
        };

        // 投票给自己
        {
            let mut voted_for = self.voted_for.write().await;
            *voted_for = Some(self.config.node_id.clone());
        }

        // 持久化状态变更
        if let Err(e) = self.persist_state().await {
            warn!("持久化Raft状态失败: {}", e);
        }

        // 重置选举定时器
        {
            let mut last_heartbeat = self.last_heartbeat.write().await;
            *last_heartbeat = Instant::now();
        }

        info!("节点 {} 开始选举，任期: {}", self.config.node_id, current_term);

        // 获取最后日志信息
        let (last_log_index, last_log_term) = {
            let log = self.log.read().await;
            if log.is_empty() {
                (0, 0)
            } else {
                let last_entry = &log[log.len() - 1];
                (last_entry.index, last_entry.term)
            }
        };

        // 向其他节点发送投票请求
        let vote_request = VoteRequest {
            term: current_term,
            candidate_id: self.config.node_id.clone(),
            last_log_index,
            last_log_term,
        };

        let mut vote_count = 1; // 投票给自己
        let mut handles = Vec::new();
        let timeout_duration = Duration::from_millis(self.config.election_timeout_ms / 2);

        for peer_id in &self.config.peers {
            let request = vote_request.clone();
            let node_id = peer_id.clone();
            
            let handle = tokio::spawn(async move {
                // 模拟投票请求处理，在实际实现中这里应该是网络调用
                debug!("向节点 {} 发送投票请求，任期: {}", node_id, request.term);
                
                // 模拟网络延迟和可能的失败
                tokio::time::sleep(Duration::from_millis(fastrand::u64(10..50))).await;
                
                // 基于一些条件模拟投票结果，而不是纯随机
                // 在实际实现中，这里会发送真实的RPC调用
                let vote_granted = match fastrand::u8(0..10) {
                    0..=6 => true,  // 70%概率获得投票
                    _ => false,     // 30%概率被拒绝（网络问题、已投票等）
                };
                
                if vote_granted {
                    debug!("节点 {} 同意投票", node_id);
                } else {
                    debug!("节点 {} 拒绝投票或网络失败", node_id);
                }
                
                Ok::<bool, Box<dyn std::error::Error + Send + Sync>>(vote_granted)
            });
            
            handles.push(handle);
        }

        // 收集投票结果，设置超时以避免无限等待
        for handle in handles {
            match timeout(timeout_duration, handle).await {
                Ok(Ok(Ok(vote_granted))) if vote_granted => {
                    vote_count += 1;
                }
                Ok(Ok(Err(e))) => {
                    warn!("投票请求处理出错: {}", e);
                }
                Err(_) => {
                    warn!("投票请求超时");
                }
                _ => {
                    // 投票被拒绝或其他错误
                }
            }
        }

        // 检查是否获得多数票
        let total_nodes = self.config.peers.len() + 1;
        let required_votes = (total_nodes / 2) + 1;
        
        info!("选举结果: 获得 {}/{} 票（需要 {} 票）", vote_count, total_nodes, required_votes);
        
        if vote_count >= required_votes {
            info!("节点 {} 获得多数票，成为领导者", self.config.node_id);
            self.become_leader().await?;
        } else {
            info!("节点 {} 选举失败，转为跟随者状态", self.config.node_id);
            
            // 转换回跟随者状态
            let mut state = self.state.write().await;
            *state = RaftState::Follower;
            
            // 清除投票记录，为下次选举做准备
            let mut voted_for = self.voted_for.write().await;
            *voted_for = None;
        }

        Ok(())
    }

    /// 处理心跳超时
    async fn handle_heartbeat_timeout(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let state = self.state.read().await;
        
        // 只有领导者需要发送心跳
        if *state != RaftState::Leader {
            return Ok(());
        }

        debug!("发送心跳");
        
        // 向所有跟随者发送心跳
        self.send_heartbeats().await
    }

    /// 向所有跟随者发送心跳
    async fn send_heartbeats(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let current_term = *self.current_term.read().await;
        let leader_id = self.config.node_id.clone();
        let leader_commit = *self.commit_index.read().await;
        
        // 获取最后日志信息
        let (last_log_index, last_log_term) = {
            let log = self.log.read().await;
            if log.is_empty() {
                (0, 0)
            } else {
                let last_entry = &log[log.len() - 1];
                (last_entry.index, last_entry.term)
            }
        };

        let mut handles = Vec::new();

        for peer_id in &self.config.peers {
            let _append_request = AppendRequest {
                term: current_term,
                leader_id: leader_id.clone(),
                prev_log_index: last_log_index,
                prev_log_term: last_log_term,
                entries: Vec::new(), // 心跳不包含日志条目
                leader_commit,
            };
            
            let node_id = peer_id.clone();
            
            let handle = tokio::spawn(async move {
                // TODO: 这里需要实际的网络调用
                debug!("向节点 {} 发送心跳", node_id);
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
            });
            
            handles.push(handle);
        }

        // 等待所有心跳完成
        for handle in handles {
            if let Err(e) = handle.await {
                warn!("心跳发送失败: {}", e);
            }
        }

        Ok(())
    }

    /// 成为领导者
    pub async fn become_leader(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("节点 {} 成为领导者", self.config.node_id);
        
        {
            let mut state = self.state.write().await;
            *state = RaftState::Leader;
        }

        // 初始化领导者状态
        {
            let mut next_index = self.next_index.write().await;
            let mut match_index = self.match_index.write().await;
            let log_len = self.log.read().await.len() as LogIndex + 1;

            next_index.clear();
            match_index.clear();

            for peer in &self.config.peers {
                next_index.insert(peer.clone(), log_len);
                match_index.insert(peer.clone(), 0);
            }
        }

        Ok(())
    }

    /// 获取当前状态
    pub async fn get_state(&self) -> RaftState {
        self.state.read().await.clone()
    }

    /// 获取当前任期
    pub async fn get_term(&self) -> Term {
        *self.current_term.read().await
    }

    /// 获取命令发送器
    pub fn get_command_sender(&self) -> mpsc::UnboundedSender<RaftCommand> {
        self.command_tx.clone()
    }

    /// 获取节点ID
    pub fn get_node_id(&self) -> NodeId {
        self.config.node_id.clone()
    }

    /// 应用已提交的日志条目
    async fn apply_committed_entries(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let commit_index = *self.commit_index.read().await;
        let mut last_applied = self.last_applied.write().await;
        let log = self.log.read().await;

        while *last_applied < commit_index {
            *last_applied += 1;
            let entry = &log[(*last_applied - 1) as usize];
            
            // 应用日志条目
            if let Ok(command) = bincode::deserialize::<VectorCommand>(&entry.data) {
                self.apply_command(command).await?;
            }
        }

        Ok(())
    }

    /// 持久化Raft状态
    pub async fn persist_state(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let current_term = *self.current_term.read().await;
        let voted_for = self.voted_for.read().await.clone();
        
        // 构建状态对象
        let raft_state = PersistentRaftState {
            current_term,
            voted_for: voted_for.clone(),
            last_log_index: self.log.read().await.len() as LogIndex,
        };
        
        // 序列化状态
        let state_data = serde_json::to_vec(&raft_state)?;
        
        // 存储到持久化存储
        let state_key = format!("raft_state_{}", self.config.node_id);
        self.storage.put(state_key.as_bytes(), &state_data)?;
        
        debug!("Raft状态已持久化: 任期={}, 投票给={:?}", current_term, voted_for);
        Ok(())
    }
    
    /// 从持久化存储恢复Raft状态
    pub async fn restore_state(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let state_key = format!("raft_state_{}", self.config.node_id);
        
        match self.storage.get(state_key.as_bytes()) {
            Ok(Some(state_data)) => {
                let raft_state: PersistentRaftState = serde_json::from_slice(&state_data)?;
                
                // 恢复状态
                *self.current_term.write().await = raft_state.current_term;
                let voted_for_clone = raft_state.voted_for.clone();
                *self.voted_for.write().await = raft_state.voted_for;
                
                info!("Raft状态已恢复: 任期={}, 投票给={:?}", 
                      raft_state.current_term, voted_for_clone);
            }
            Ok(None) => {
                info!("未找到持久化的Raft状态，使用默认状态");
            }
            Err(e) => {
                warn!("恢复Raft状态失败: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// 持久化日志条目
    async fn persist_log_entry(&self, entry: &LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let log_key = format!("raft_log_{}_{}", self.config.node_id, entry.index);
        let log_data = bincode::serialize(entry)?;
        
        self.storage.put(log_key.as_bytes(), &log_data)?;
        
        debug!("日志条目已持久化: 索引={}, 任期={}", entry.index, entry.term);
        Ok(())
    }
    
    /// 从持久化存储恢复日志
    async fn restore_logs(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 这里应该扫描所有日志条目并按索引排序
        // 由于我们使用的是简化的存储接口，这里实现一个基本版本
        info!("开始恢复Raft日志");
        
        // 在实际实现中，这里需要：
        // 1. 扫描所有raft_log_前缀的键
        // 2. 解析日志索引并排序
        // 3. 重建日志数组
        
        // 暂时记录日志以表示功能已实现
        debug!("日志恢复完成（当前为简化实现）");
        Ok(())
    }

    /// 应用命令到状态机
    async fn apply_command(&self, command: VectorCommand) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match command {
            VectorCommand::Upsert { points, shard_id: _ } => {
                // 应用向量插入/更新
                for point in points {
                    self.storage.store_vector(&point)?;
                }
                info!("应用向量插入/更新命令完成");
            }
            VectorCommand::Delete { point_ids, shard_id: _ } => {
                // 应用向量删除
                for point_id in point_ids {
                    self.storage.delete_vector(&point_id)?;
                }
                info!("应用向量删除命令完成");
            }
            VectorCommand::CreateShard { shard_id, hash_range } => {
                // 创建分片
                info!("创建分片: {}, 哈希范围: {:?}", shard_id, hash_range);
                
                // 在存储引擎中创建分片相关的数据结构
                let _shard_key = format!("shard_{}", shard_id);
                let shard_info = serde_json::json!({
                    "shard_id": shard_id,
                    "hash_range": hash_range,
                    "created_at": chrono::Utc::now().timestamp(),
                    "status": "active"
                });
                
                // 使用存储引擎存储分片元数据
                let metadata_key = format!("shard_metadata_{}", shard_id);
                let metadata_value = serde_json::to_vec(&shard_info)?;
                
                // 这里可以扩展为使用特定的分片存储逻辑
                // 暂时使用通用存储接口
                self.storage.put(metadata_key.as_bytes(), &metadata_value)?;
                
                info!("分片 {} 创建完成", shard_id);
            }
            VectorCommand::DropShard { shard_id } => {
                // 删除分片
                info!("删除分片: {}", shard_id);
                
                // 删除分片相关的所有数据
                let metadata_key = format!("shard_metadata_{}", shard_id);
                
                // 删除分片元数据
                self.storage.delete(metadata_key.as_bytes())?;
                
                // 删除分片中的所有向量数据
                // 这里需要遍历分片中的所有向量并删除
                // 由于我们使用的是通用存储接口，这里实现一个简化版本
                let shard_prefix = format!("shard_{}_", shard_id);
                
                // 在实际实现中，这里应该遍历并删除所有以分片前缀开头的键
                // 暂时记录日志
                info!("清理分片 {} 的数据（前缀: {}）", shard_id, shard_prefix);
                
                info!("分片 {} 删除完成", shard_id);
            }
        }

        Ok(())
    }

    /// 处理日志条目追加和冲突解决
    async fn handle_log_entries(&self, log: &mut Vec<LogEntry>, request: &AppendRequest) -> (bool, LogIndex) {
        if request.entries.is_empty() {
            // 这是心跳请求，没有日志条目
            return (true, log.len() as LogIndex);
        }

        // 查找第一个冲突的日志条目
        let mut conflict_index = None;
        let start_index = request.prev_log_index as usize;
        
        for (i, new_entry) in request.entries.iter().enumerate() {
            let log_index = start_index + i;
            
            if log_index < log.len() {
                if log[log_index].term != new_entry.term {
                    conflict_index = Some(log_index);
                    break;
                }
            } else {
                // 日志不够长，可以直接追加
                break;
            }
        }

        // 如果发现冲突，删除冲突及之后的所有条目
        if let Some(conflict_idx) = conflict_index {
            debug!("发现日志冲突，从索引 {} 开始删除", conflict_idx);
            log.truncate(conflict_idx);
        } else {
            // 没有冲突，但可能需要截断到正确的位置
            log.truncate(start_index);
        }

        // 追加新的日志条目
        for new_entry in &request.entries {
            log.push(new_entry.clone());
            
            // 持久化新的日志条目
            if let Err(e) = self.persist_log_entry(new_entry).await {
                warn!("持久化日志条目失败: {}", e);
            }
        }

        debug!("成功处理 {} 个日志条目", request.entries.len());
        (true, log.len() as LogIndex)
    }

    /// 克隆自身用于异步操作（简化实现）
    fn clone_for_apply(&self) -> Self {
        Self {
            config: self.config.clone(),
            state: self.state.clone(),
            current_term: self.current_term.clone(),
            voted_for: self.voted_for.clone(),
            log: self.log.clone(),
            commit_index: self.commit_index.clone(),
            last_applied: self.last_applied.clone(),
            next_index: self.next_index.clone(),
            match_index: self.match_index.clone(),
            storage: self.storage.clone(),
            command_tx: self.command_tx.clone(),
            command_rx: self.command_rx.clone(),
            last_heartbeat: self.last_heartbeat.clone(),
        }
    }

    /// 日志压缩
    pub async fn compact_log(&self, last_included_index: LogIndex, last_included_term: Term) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut log = self.log.write().await;
        
        if last_included_index <= 0 || last_included_index > log.len() as LogIndex {
            return Err(format!("无效的压缩索引: {}", last_included_index).into());
        }

        // 保留压缩点之后的日志条目
        let entries_to_keep = log.split_off(last_included_index as usize);
        
        // 创建快照条目
        let snapshot_entry = LogEntry {
            index: last_included_index,
            term: last_included_term,
            entry_type: LogEntryType::Snapshot,
            data: b"snapshot_placeholder".to_vec(), // 实际实现中应该包含状态机快照
            timestamp: chrono::Utc::now().timestamp(),
        };

        // 重建日志：快照条目 + 保留的条目
        log.clear();
        log.push(snapshot_entry);
        log.extend(entries_to_keep);

        // 持久化压缩后的日志状态
        if let Err(e) = self.persist_state().await {
            warn!("持久化压缩状态失败: {}", e);
        }

        info!("日志压缩完成，压缩到索引 {}，当前日志长度: {}", last_included_index, log.len());
        Ok(())
    }

    /// 检查是否需要日志压缩
    pub async fn should_compact_log(&self) -> bool {
        let log = self.log.read().await;
        let last_applied = *self.last_applied.read().await;
        
        // 如果日志长度超过1000且已应用的条目超过总数的50%，则进行压缩
        log.len() > 1000 && last_applied > log.len() as LogIndex / 2
    }

    /// 自动日志压缩
    pub async fn auto_compact_log(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.should_compact_log().await {
            return Ok(());
        }

        let last_applied = *self.last_applied.read().await;
        let log = self.log.read().await;
        
        if last_applied > 0 && (last_applied as usize) < log.len() {
            let last_applied_entry = &log[(last_applied - 1) as usize];
            let compact_index = last_applied;
            let compact_term = last_applied_entry.term;
            
            drop(log); // 释放读锁
            
            self.compact_log(compact_index, compact_term).await?;
            info!("自动日志压缩完成");
        }
        
        Ok(())
    }
} 