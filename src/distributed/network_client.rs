use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error};
use chrono::Utc;
use uuid;

use crate::types::{NodeId, Point, SearchRequest, SearchResult};

/// 分布式网络客户端
#[derive(Clone)]
pub struct DistributedNetworkClient {
    /// HTTP 客户端
    http_client: reqwest::Client,
    /// 请求超时时间
    timeout: Duration,
    /// 重试次数
    retry_attempts: u32,
    /// 本地节点ID（企业级节点身份管理）
    local_node_id: NodeId,
}

impl DistributedNetworkClient {
    /// 创建新的网络客户端
    pub fn new() -> Self {
        Self::with_node_id(Self::generate_node_id())
    }
    
    /// 使用指定节点ID创建网络客户端（企业级）
    pub fn with_node_id(node_id: NodeId) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("创建HTTP客户端失败");

        Self {
            http_client,
            timeout: Duration::from_secs(10),
            retry_attempts: 3,
            local_node_id: node_id,
        }
    }
    
    /// 生成企业级节点ID（基于MAC地址和时间戳）
    fn generate_node_id() -> NodeId {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        // 获取主机名作为节点标识的一部分
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
            
        // 获取时间戳确保唯一性
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        // 生成企业级节点ID格式：hostname-timestamp-random
        format!("{}-{}-{:x}", 
            hostname, 
            timestamp, 
            fastrand::u64(..)
        )
    }
    
    /// 获取本地节点ID
    pub fn get_local_node_id(&self) -> &NodeId {
        &self.local_node_id
    }

    /// 配置超时时间
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 配置重试次数
    pub fn with_retry_attempts(mut self, attempts: u32) -> Self {
        self.retry_attempts = attempts;
        self
    }

    /// 发送心跳请求
    pub async fn send_heartbeat(
        &self,
        target_node: &NodeId,
        node_address: &str,
    ) -> Result<HeartbeatResponse, NetworkError> {
        let url = format!("http://{}/api/v1/heartbeat", node_address);
        let request = HeartbeatRequest {
            sender_id: self.local_node_id.clone(), // 企业级节点ID管理
            timestamp: Utc::now().timestamp(),
        };

        for attempt in 0..self.retry_attempts {
            match self.send_post_request(&url, &request).await {
                Ok(response) => {
                    debug!("心跳请求成功，目标节点: {}", target_node);
                    return Ok(response);
                }
                Err(e) => {
                    if attempt < self.retry_attempts - 1 {
                        tokio::time::sleep(Duration::from_millis(100 * (attempt + 1) as u64)).await;
                    } else {
                        error!("心跳请求最终失败，目标节点: {}: {}", target_node, e);
                        return Err(NetworkError::RequestFailed(e.to_string()));
                    }
                }
            }
        }

        Err(NetworkError::MaxRetriesExceeded)
    }

    /// 发送数据复制请求
    pub async fn send_replication_data(
        &self,
        target_node: &NodeId,
        node_address: &str,
        data: &[u8],
    ) -> Result<ReplicationResponse, NetworkError> {
        let url = format!("http://{}/api/v1/replicate", node_address);
        let request = ReplicationRequest {
            data: data.to_vec(),
            sender_id: "local_node".to_string(),
            timestamp: Utc::now().timestamp(),
        };

        for attempt in 0..self.retry_attempts {
            match self.send_post_request(&url, &request).await {
                Ok(response) => {
                    debug!("数据复制成功，目标节点: {}, 数据大小: {} 字节", target_node, data.len());
                    return Ok(response);
                }
                Err(e) => {
                    if attempt < self.retry_attempts - 1 {
                        tokio::time::sleep(Duration::from_millis(200 * (attempt + 1) as u64)).await;
                    } else {
                        error!("数据复制最终失败，目标节点: {}: {}", target_node, e);
                        return Err(NetworkError::RequestFailed(e.to_string()));
                    }
                }
            }
        }

        Err(NetworkError::MaxRetriesExceeded)
    }

    /// 发送向量插入请求
    pub async fn send_vector_insert(
        &self,
        target_node: &NodeId,
        node_address: &str,
        point: &Point,
    ) -> Result<VectorInsertResponse, NetworkError> {
        let url = format!("http://{}/api/v1/vectors", node_address);
        let request = VectorInsertRequest {
            point: point.clone(),
            sender_id: "local_node".to_string(),
        };

        match self.send_post_request(&url, &request).await {
            Ok(response) => {
                debug!("向量插入成功，目标节点: {}, 向量ID: {}", target_node, point.id);
                Ok(response)
            }
            Err(e) => {
                error!("向量插入失败，目标节点: {}: {}", target_node, e);
                Err(NetworkError::RequestFailed(e.to_string()))
            }
        }
    }

    /// 发送向量删除请求
    pub async fn send_vector_delete(
        &self,
        target_node: &NodeId,
        node_address: &str,
        point_id: &str,
    ) -> Result<VectorDeleteResponse, NetworkError> {
        let url = format!("http://{}/api/v1/vectors/{}", node_address, point_id);

        match self.send_delete_request(&url).await {
            Ok(response) => {
                debug!("向量删除成功，目标节点: {}, 向量ID: {}", target_node, point_id);
                Ok(response)
            }
            Err(e) => {
                error!("向量删除失败，目标节点: {}: {}", target_node, e);
                Err(NetworkError::RequestFailed(e.to_string()))
            }
        }
    }

    /// 发送搜索请求（企业级分布式搜索）
    pub async fn send_search_request(
        &self,
        target_node: &NodeId,
        node_address: &str,
        search_request: &SearchRequest,
    ) -> Result<Vec<SearchResult>, NetworkError> {
        let url = format!("http://{}/api/v1/search", node_address);
        
        // 构建企业级搜索请求，包含节点身份和负载均衡信息
        let distributed_request = DistributedSearchRequest {
            sender_id: self.local_node_id.clone(),
            target_node: target_node.clone(),
            search_params: search_request.clone(),
            timestamp: Utc::now().timestamp(),
            request_id: uuid::Uuid::new_v4().to_string(),
            priority: SearchPriority::Normal,
        };
        
        // 企业级重试逻辑和容错处理
        for attempt in 0..self.retry_attempts {
            match self.send_post_request(&url, &distributed_request).await {
                Ok(response) => {
                    debug!(
                        "分布式搜索请求成功，目标节点: {}, 请求ID: {}", 
                        target_node, 
                        distributed_request.request_id
                    );
                    
                    // 记录企业级性能指标
                    metrics::counter!("distributed_search_requests_total")
                        .increment(1);
                    metrics::gauge!("distributed_search_latency_ms")
                        .set((Utc::now().timestamp() - distributed_request.timestamp) as f64);
                    
                    return Ok(response);
                }
                Err(e) => {
                    error!(
                        "分布式搜索请求失败，目标节点: {}, 尝试: {}/{}, 错误: {}", 
                        target_node, attempt + 1, self.retry_attempts, e
                    );
                    
                    if attempt < self.retry_attempts - 1 {
                        // 企业级指数退避重试策略
                        let delay = Duration::from_millis(100 * (2_u64.pow(attempt as u32)));
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        
        // 记录失败指标
        metrics::counter!("distributed_search_failures_total")
            .increment(1);
            
        Err(NetworkError::RequestTimeout(format!(
            "分布式搜索请求超时，目标节点: {}, 重试次数: {}", 
            target_node, self.retry_attempts
        )))
    }

    /// 发送分片迁移请求
    pub async fn send_shard_migration(
        &self,
        target_node: &NodeId,
        node_address: &str,
        shard_data: &[u8],
    ) -> Result<ShardMigrationResponse, NetworkError> {
        let url = format!("http://{}/api/v1/shards/migrate", node_address);
        let request = ShardMigrationRequest {
            shard_data: shard_data.to_vec(),
            sender_id: "local_node".to_string(),
        };

        match self.send_post_request(&url, &request).await {
            Ok(response) => {
                debug!("分片迁移成功，目标节点: {}, 数据大小: {} 字节", target_node, shard_data.len());
                Ok(response)
            }
            Err(e) => {
                error!("分片迁移失败，目标节点: {}: {}", target_node, e);
                Err(NetworkError::RequestFailed(e.to_string()))
            }
        }
    }

    /// 发送健康检查请求
    pub async fn send_health_check(
        &self,
        target_node: &NodeId,
        node_address: &str,
    ) -> Result<HealthCheckResponse, NetworkError> {
        let url = format!("http://{}/api/v1/health", node_address);

        match self.send_get_request(&url).await {
            Ok(response) => {
                debug!("健康检查成功，目标节点: {}", target_node);
                Ok(response)
            }
            Err(e) => {
                error!("健康检查失败，目标节点: {}: {}", target_node, e);
                Err(NetworkError::RequestFailed(e.to_string()))
            }
        }
    }

    /// 发送POST请求
    async fn send_post_request<T, R>(&self, url: &str, request: &T) -> Result<R, Box<dyn std::error::Error + Send + Sync>>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let response = self
            .http_client
            .post(url)
            .timeout(self.timeout)
            .json(request)
            .send()
            .await?;

        if response.status().is_success() {
            let result = response.json::<R>().await?;
            Ok(result)
        } else {
            Err(format!("HTTP请求失败，状态码: {}", response.status()).into())
        }
    }

    /// 发送GET请求
    async fn send_get_request<R>(&self, url: &str) -> Result<R, Box<dyn std::error::Error + Send + Sync>>
    where
        R: for<'de> Deserialize<'de>,
    {
        let response = self
            .http_client
            .get(url)
            .timeout(self.timeout)
            .send()
            .await?;

        if response.status().is_success() {
            let result = response.json::<R>().await?;
            Ok(result)
        } else {
            Err(format!("HTTP请求失败，状态码: {}", response.status()).into())
        }
    }

    /// 发送DELETE请求
    async fn send_delete_request<R>(&self, url: &str) -> Result<R, Box<dyn std::error::Error + Send + Sync>>
    where
        R: for<'de> Deserialize<'de>,
    {
        let response = self
            .http_client
            .delete(url)
            .timeout(self.timeout)
            .send()
            .await?;

        if response.status().is_success() {
            let result = response.json::<R>().await?;
            Ok(result)
        } else {
            Err(format!("HTTP请求失败，状态码: {}", response.status()).into())
        }
    }
}

impl Default for DistributedNetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 网络错误类型
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("请求失败: {0}")]
    RequestFailed(String),
    #[error("超过最大重试次数")]
    MaxRetriesExceeded,
    #[error("连接超时")]
    Timeout,
    #[error("节点不可达")]
    NodeUnreachable,
}

/// 心跳请求
#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub sender_id: String,
    pub timestamp: i64,
}

/// 心跳响应
#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub node_id: String,
    pub timestamp: i64,
    pub load_info: Option<NodeLoadInfo>,
}

/// 节点负载信息
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeLoadInfo {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub shard_count: u32,
    pub qps: f64,
}

/// 复制请求
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicationRequest {
    pub data: Vec<u8>,
    pub sender_id: String,
    pub timestamp: i64,
}

/// 复制响应
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicationResponse {
    pub success: bool,
    pub bytes_received: usize,
    pub timestamp: i64,
}

/// 向量插入请求
#[derive(Debug, Serialize, Deserialize)]
pub struct VectorInsertRequest {
    pub point: Point,
    pub sender_id: String,
}

/// 向量插入响应
#[derive(Debug, Serialize, Deserialize)]
pub struct VectorInsertResponse {
    pub success: bool,
    pub point_id: String,
}

/// 向量删除响应
#[derive(Debug, Serialize, Deserialize)]
pub struct VectorDeleteResponse {
    pub success: bool,
    pub point_id: String,
}

/// 分片迁移请求
#[derive(Debug, Serialize, Deserialize)]
pub struct ShardMigrationRequest {
    pub shard_data: Vec<u8>,
    pub sender_id: String,
}

/// 分片迁移响应
#[derive(Debug, Serialize, Deserialize)]
pub struct ShardMigrationResponse {
    pub success: bool,
    pub bytes_received: usize,
}

/// 健康检查响应
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub is_healthy: bool,
    pub node_id: String,
    pub uptime_seconds: u64,
    pub load_info: NodeLoadInfo,
}

/// 企业级分布式搜索请求
#[derive(Debug, Serialize, Deserialize)]
pub struct DistributedSearchRequest {
    pub sender_id: NodeId,
    pub target_node: NodeId,
    pub search_params: SearchRequest,
    pub timestamp: i64,
    pub request_id: String,
    pub priority: SearchPriority,
}

/// 搜索优先级（企业级负载管理）
#[derive(Debug, Serialize, Deserialize)]
pub enum SearchPriority {
    Low,
    Normal,
    High,
    Critical,
}