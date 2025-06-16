use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc, oneshot};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};

use crate::distributed::raft::{RaftNode, VoteRequest, VoteResponse, AppendRequest, AppendResponse};
use crate::types::{NodeId, NodeInfo, ClusterInfo, HeartbeatMessage};

/// 网络管理器
pub struct NetworkManager {
    /// 本地节点信息
    local_node: NodeInfo,
    /// 远程节点连接
    connections: Arc<RwLock<HashMap<NodeId, NodeConnection>>>,
    /// HTTP 客户端
    http_client: reqwest::Client,
    /// Raft 节点
    raft_node: Arc<RaftNode>,
}

/// 节点连接
pub struct NodeConnection {
    /// 节点信息
    pub node_info: NodeInfo,
    /// 基础URL
    pub base_url: String,
    /// 连接状态
    pub status: ConnectionStatus,
    /// 最后活跃时间
    pub last_active: std::time::Instant,
}

/// 连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    /// 连接中
    Connecting,
    /// 已连接
    Connected,
    /// 断开连接
    Disconnected,
    /// 连接失败
    Failed,
}

/// HTTP API 请求/响应类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinClusterRequest {
    pub node_info: NodeInfo,
    pub cluster_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinClusterResponse {
    pub cluster_info: ClusterInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub node_id: NodeId,
    pub load: crate::types::NodeLoad,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub cluster_info: Option<ClusterInfo>,
}

impl NetworkManager {
    /// 创建新的网络管理器
    pub fn new(local_node: NodeInfo, raft_node: Arc<RaftNode>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("创建HTTP客户端失败");

        Self {
            local_node,
            connections: Arc::new(RwLock::new(HashMap::new())),
            http_client,
            raft_node,
        }
    }

    /// 启动网络服务
    pub async fn start(&mut self, bind_address: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("启动网络服务，绑定地址: {}", bind_address);

        // TODO: 启动 HTTP 服务器
        // 这里可以使用 axum 或 warp 来创建 HTTP API 服务器
        
        Ok(())
    }

    /// 连接到远程节点
    pub async fn connect_to_node(&self, node_info: NodeInfo) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let node_id = node_info.id.clone();
        info!("连接到节点: {}", node_id);

        // 构建基础URL
        let base_url = format!("http://{}:{}", node_info.address, node_info.port);

        // 测试连接
        let health_url = format!("{}/health", base_url);
        match self.http_client.get(&health_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let connection = NodeConnection {
                        node_info,
                        base_url,
                        status: ConnectionStatus::Connected,
                        last_active: std::time::Instant::now(),
                    };

                    // 保存连接
                    self.connections.write().await.insert(node_id.clone(), connection);
                    info!("成功连接到节点: {}", node_id);
                } else {
                    warn!("节点健康检查失败: {} - {}", node_id, response.status());
                    return Err(format!("节点健康检查失败: {}", response.status()).into());
                }
            }
            Err(e) => {
                error!("连接节点失败: {} - {}", node_id, e);
                return Err(e.into());
            }
        }

        Ok(())
    }

    /// 断开与节点的连接
    pub async fn disconnect_from_node(&self, node_id: &NodeId) {
        info!("断开与节点的连接: {}", node_id);
        
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(node_id) {
            connection.status = ConnectionStatus::Disconnected;
        }
    }

    /// 发送 Raft 投票请求
    pub async fn send_vote_request(&self, node_id: &NodeId, request: VoteRequest) -> Result<VoteResponse, Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        
        if let Some(connection) = connections.get(node_id) {
            let url = format!("{}/raft/vote", connection.base_url);
            
            let response = self.http_client
                .post(&url)
                .json(&request)
                .send()
                .await?;

            if response.status().is_success() {
                let api_response: ApiResponse<VoteResponse> = response.json().await?;
                if api_response.success {
                    if let Some(vote_response) = api_response.data {
                        return Ok(vote_response);
                    }
                }
                return Err(api_response.error.unwrap_or("未知错误".to_string()).into());
            } else {
                return Err(format!("HTTP错误: {}", response.status()).into());
            }
        } else {
            Err(format!("未找到节点连接: {}", node_id).into())
        }
    }

    /// 发送 Raft 追加日志请求
    pub async fn send_append_request(&self, node_id: &NodeId, request: AppendRequest) -> Result<AppendResponse, Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        
        if let Some(connection) = connections.get(node_id) {
            let url = format!("{}/raft/append", connection.base_url);
            
            let response = self.http_client
                .post(&url)
                .json(&request)
                .send()
                .await?;

            if response.status().is_success() {
                let api_response: ApiResponse<AppendResponse> = response.json().await?;
                if api_response.success {
                    if let Some(append_response) = api_response.data {
                        return Ok(append_response);
                    }
                }
                return Err(api_response.error.unwrap_or("未知错误".to_string()).into());
            } else {
                return Err(format!("HTTP错误: {}", response.status()).into());
            }
        } else {
            Err(format!("未找到节点连接: {}", node_id).into())
        }
    }

    /// 发送心跳
    pub async fn send_heartbeat(&self, node_id: &NodeId, heartbeat: HeartbeatMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        
        if let Some(connection) = connections.get(node_id) {
            let url = format!("{}/cluster/heartbeat", connection.base_url);
            
            let request = HeartbeatRequest {
                node_id: heartbeat.node_id,
                load: heartbeat.load,
                timestamp: heartbeat.timestamp,
            };
            
            let response = self.http_client
                .post(&url)
                .json(&request)
                .send()
                .await?;

            if response.status().is_success() {
                let api_response: ApiResponse<HeartbeatResponse> = response.json().await?;
                if !api_response.success {
                    warn!("心跳响应失败: {:?}", api_response.error);
                }
            } else {
                warn!("心跳HTTP错误: {}", response.status());
            }
            
            Ok(())
        } else {
            Err(format!("未找到节点连接: {}", node_id).into())
        }
    }

    /// 加入集群
    pub async fn join_cluster(&self, seed_node: &NodeInfo, cluster_token: String) -> Result<ClusterInfo, Box<dyn std::error::Error + Send + Sync>> {
        info!("尝试加入集群，种子节点: {}", seed_node.id);

        let base_url = format!("http://{}:{}", seed_node.address, seed_node.port);
        let url = format!("{}/cluster/join", base_url);

        let request = JoinClusterRequest {
            node_info: self.local_node.clone(),
            cluster_token,
        };

        let response = self.http_client
            .post(&url)
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            let api_response: ApiResponse<JoinClusterResponse> = response.json().await?;
            if api_response.success {
                if let Some(join_response) = api_response.data {
                    info!("成功加入集群");
                    return Ok(join_response.cluster_info);
                }
            }
            return Err(api_response.error.unwrap_or("加入集群失败".to_string()).into());
        } else {
            return Err(format!("HTTP错误: {}", response.status()).into());
        }
    }

    /// 获取集群信息
    pub async fn get_cluster_info(&self, node_id: &NodeId) -> Result<ClusterInfo, Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        
        if let Some(connection) = connections.get(node_id) {
            let url = format!("{}/cluster/info", connection.base_url);
            
            let response = self.http_client
                .get(&url)
                .send()
                .await?;

            if response.status().is_success() {
                let api_response: ApiResponse<ClusterInfo> = response.json().await?;
                if api_response.success {
                    if let Some(cluster_info) = api_response.data {
                        return Ok(cluster_info);
                    }
                }
                return Err(api_response.error.unwrap_or("获取集群信息失败".to_string()).into());
            } else {
                return Err(format!("HTTP错误: {}", response.status()).into());
            }
        } else {
            Err(format!("未找到节点连接: {}", node_id).into())
        }
    }

    /// 获取连接状态
    pub async fn get_connection_status(&self, node_id: &NodeId) -> Option<ConnectionStatus> {
        self.connections.read().await.get(node_id).map(|conn| conn.status.clone())
    }

    /// 获取所有连接
    pub async fn get_all_connections(&self) -> HashMap<NodeId, ConnectionStatus> {
        self.connections.read().await.iter()
            .map(|(id, conn)| (id.clone(), conn.status.clone()))
            .collect()
    }

    /// 检查连接健康状态
    pub async fn check_connections_health(&self) {
        let mut connections = self.connections.write().await;
        
        for (node_id, connection) in connections.iter_mut() {
            if connection.status == ConnectionStatus::Connected {
                let health_url = format!("{}/health", connection.base_url);
                
                match self.http_client.get(&health_url).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            connection.last_active = std::time::Instant::now();
                        } else {
                            warn!("节点 {} 健康检查失败: {}", node_id, response.status());
                            connection.status = ConnectionStatus::Failed;
                        }
                    }
                    Err(e) => {
                        warn!("节点 {} 连接检查失败: {}", node_id, e);
                        connection.status = ConnectionStatus::Failed;
                    }
                }
            }
        }
    }

    /// 启动连接监控
    pub async fn start_connection_monitoring(&self) {
        let network_manager = Arc::new(self.clone_for_monitoring());
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                network_manager.check_connections_health().await;
            }
        });
    }

    /// 为监控克隆网络管理器
    fn clone_for_monitoring(&self) -> NetworkManagerMonitor {
        NetworkManagerMonitor {
            connections: self.connections.clone(),
            http_client: self.http_client.clone(),
        }
    }

    /// 关闭网络服务
    pub async fn shutdown(&mut self) {
        info!("关闭网络服务");
        
        // 关闭所有连接
        let mut connections = self.connections.write().await;
        for (node_id, connection) in connections.iter_mut() {
            connection.status = ConnectionStatus::Disconnected;
            debug!("关闭与节点 {} 的连接", node_id);
        }
    }
}

/// 监控专用的网络管理器
struct NetworkManagerMonitor {
    connections: Arc<RwLock<HashMap<NodeId, NodeConnection>>>,
    http_client: reqwest::Client,
}

impl NetworkManagerMonitor {
    async fn check_connections_health(&self) {
        let mut connections = self.connections.write().await;
        
        for (node_id, connection) in connections.iter_mut() {
            if connection.status == ConnectionStatus::Connected {
                let health_url = format!("{}/health", connection.base_url);
                
                match self.http_client.get(&health_url).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            connection.last_active = std::time::Instant::now();
                        } else {
                            warn!("节点 {} 健康检查失败: {}", node_id, response.status());
                            connection.status = ConnectionStatus::Failed;
                        }
                    }
                    Err(e) => {
                        warn!("节点 {} 连接检查失败: {}", node_id, e);
                        connection.status = ConnectionStatus::Failed;
                    }
                }
            }
        }
    }
}

/// HTTP API 服务器
pub struct ApiServer {
    /// 本地节点信息
    local_node: NodeInfo,
    /// Raft 节点
    raft_node: Arc<RaftNode>,
    /// 集群信息
    cluster_info: Arc<RwLock<ClusterInfo>>,
}

impl ApiServer {
    /// 创建新的API服务器
    pub fn new(
        local_node: NodeInfo,
        raft_node: Arc<RaftNode>,
        cluster_info: Arc<RwLock<ClusterInfo>>,
    ) -> Self {
        Self {
            local_node,
            raft_node,
            cluster_info,
        }
    }

    /// 启动API服务器
    pub async fn start(&self, bind_address: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("启动API服务器，绑定地址: {}", bind_address);

        // TODO: 使用 axum 或 warp 实现 HTTP API 服务器
        // 这里需要实现以下端点：
        // - GET /health - 健康检查
        // - POST /cluster/join - 加入集群
        // - GET /cluster/info - 获取集群信息
        // - POST /cluster/heartbeat - 心跳
        // - POST /raft/vote - Raft投票
        // - POST /raft/append - Raft追加日志
        // - POST /vector/upsert - 向量插入
        // - POST /vector/search - 向量搜索
        // - DELETE /vector/delete - 向量删除

        Ok(())
    }
} 