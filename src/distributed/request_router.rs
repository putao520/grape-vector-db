use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

use crate::distributed::load_balancer::{IntelligentLoadBalancer, LoadBalancerError};
use crate::distributed::network_client::DistributedNetworkClient;
use crate::types::*;
use thiserror::Error;

/// 请求路由错误类型
#[derive(Error, Debug)]
pub enum RequestRouterError {
    #[error("负载均衡器错误: {0}")]
    LoadBalancer(#[from] LoadBalancerError),
    
    #[error("网络错误: {0}")]
    Network(#[from] VectorDbError),
    
    #[error("缓存错误: {0}")]
    Cache(String),
    
    #[error("配置错误: {0}")]
    Configuration(String),
    
    #[error("超时错误: 请求在 {timeout_ms}ms 内未完成")]
    Timeout { timeout_ms: u64 },
    
    #[error("所有节点都不可用")]
    AllNodesUnavailable,
}

/// 请求类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequestType {
    /// 向量搜索
    VectorSearch,
    /// 文档插入
    DocumentInsert,
    /// 文档删除
    DocumentDelete,
    /// 批量操作
    BatchOperation,
    /// 健康检查
    HealthCheck,
    /// 集群信息
    ClusterInfo,
}

/// 请求路由信息
#[derive(Debug, Clone)]
pub struct RequestRouteInfo {
    /// 请求ID
    pub request_id: String,
    /// 请求类型
    pub request_type: RequestType,
    /// 请求大小（字节）
    pub request_size_bytes: u64,
    /// 优先级 (1-10, 10最高)
    pub priority: u8,
    /// 超时时间
    pub timeout: Duration,
    /// 是否需要一致性保证
    pub requires_consistency: bool,
}

/// 路由策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// 是否启用故障转移
    pub enable_failover: bool,
    /// 重试次数
    pub max_retries: u32,
    /// 请求超时
    pub request_timeout_ms: u64,
    /// 连接池大小
    pub connection_pool_size: u32,
    /// 是否启用请求缓存
    pub enable_request_cache: bool,
    /// 缓存TTL (秒)
    pub cache_ttl_secs: u64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            enable_failover: true,
            max_retries: 3,
            request_timeout_ms: 5000,
            connection_pool_size: 10,
            enable_request_cache: true,
            cache_ttl_secs: 300,
        }
    }
}

/// 路由执行结果
#[derive(Debug)]
pub struct RouteExecutionResult<T> {
    /// 响应数据
    pub response: T,
    /// 执行节点
    pub executed_node: NodeId,
    /// 执行时间 (毫秒)
    pub execution_time_ms: u64,
    /// 是否使用了故障转移
    pub used_failover: bool,
    /// 重试次数
    pub retry_count: u32,
}

/// 集群感知的请求路由器
pub struct ClusterAwareRequestRouter {
    /// 负载均衡器
    load_balancer: Arc<IntelligentLoadBalancer>,
    /// 网络客户端
    network_client: Arc<DistributedNetworkClient>,
    /// 路由配置
    config: RoutingConfig,
    /// 节点连接池
    connection_pools: Arc<RwLock<HashMap<NodeId, Arc<ConnectionPool>>>>,
    /// 搜索请求缓存
    search_cache: TypedCache<GrpcSearchResponse>,
    /// 插入请求缓存
    insert_cache: TypedCache<String>,
    /// 路由性能统计
    routing_metrics: Arc<RwLock<RoutingMetrics>>,
}

/// 连接池
#[derive(Debug)]
pub struct ConnectionPool {
    /// 节点ID
    pub node_id: NodeId,
    /// 活跃连接数
    pub active_connections: u32,
    /// 最大连接数
    pub max_connections: u32,
    /// 最后使用时间
    pub last_used: Instant,
}

/// 缓存响应
#[derive(Debug, Clone)]
pub struct CachedResponse<T> {
    /// 响应数据
    pub data: T,
    /// 缓存时间
    pub cached_at: Instant,
    /// TTL
    pub ttl: Duration,
}

/// 类型化的缓存存储
pub struct TypedCache<T> {
    /// 缓存存储
    storage: Arc<RwLock<HashMap<String, CachedResponse<T>>>>,
    /// 默认TTL
    default_ttl: Duration,
}

impl<T: Clone> TypedCache<T> {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
            default_ttl,
        }
    }

    pub async fn get(&self, key: &str) -> Option<T> {
        let storage = self.storage.read().await;
        if let Some(cached) = storage.get(key) {
            if cached.cached_at.elapsed() < cached.ttl {
                return Some(cached.data.clone());
            }
        }
        None
    }

    pub async fn set(&self, key: String, value: T) {
        self.set_with_ttl(key, value, self.default_ttl).await;
    }

    pub async fn set_with_ttl(&self, key: String, value: T, ttl: Duration) {
        let mut storage = self.storage.write().await;
        storage.insert(key, CachedResponse {
            data: value,
            cached_at: Instant::now(),
            ttl,
        });
    }

    pub async fn remove(&self, key: &str) {
        let mut storage = self.storage.write().await;
        storage.remove(key);
    }

    pub async fn clear_expired(&self) {
        let mut storage = self.storage.write().await;
        let now = Instant::now();
        storage.retain(|_, cached| now.duration_since(cached.cached_at) < cached.ttl);
    }
}

/// 路由性能指标
#[derive(Debug, Clone, Default)]
pub struct RoutingMetrics {
    /// 总请求数
    pub total_requests: u64,
    /// 成功请求数
    pub successful_requests: u64,
    /// 失败请求数
    pub failed_requests: u64,
    /// 故障转移次数
    pub failover_count: u64,
    /// 平均响应时间 (毫秒)
    pub avg_response_time_ms: f64,
    /// 缓存命中数
    pub cache_hits: u64,
    /// 缓存未命中数
    pub cache_misses: u64,
    /// 按节点的请求分布
    pub requests_per_node: HashMap<NodeId, u64>,
}

impl ClusterAwareRequestRouter {
    /// 创建新的集群感知请求路由器
    pub fn new(
        load_balancer: Arc<IntelligentLoadBalancer>,
        network_client: Arc<DistributedNetworkClient>,
        config: RoutingConfig,
    ) -> Result<Self, RequestRouterError> {
        // 配置验证
        Self::validate_config(&config)?;

        let cache_ttl = Duration::from_secs(config.cache_ttl_secs);
        
        Ok(Self {
            load_balancer,
            network_client,
            config,
            connection_pools: Arc::new(RwLock::new(HashMap::new())),
            search_cache: TypedCache::new(cache_ttl),
            insert_cache: TypedCache::new(cache_ttl),
            routing_metrics: Arc::new(RwLock::new(RoutingMetrics::default())),
        })
    }

    /// 验证配置
    fn validate_config(config: &RoutingConfig) -> Result<(), RequestRouterError> {
        if config.max_retries > 10 {
            return Err(RequestRouterError::Configuration(
                "最大重试次数不能超过10".to_string()
            ));
        }

        if config.request_timeout_ms > 300_000 {
            return Err(RequestRouterError::Configuration(
                "请求超时不能超过5分钟".to_string()
            ));
        }

        if config.connection_pool_size == 0 || config.connection_pool_size > 1000 {
            return Err(RequestRouterError::Configuration(
                "连接池大小必须在1-1000范围内".to_string()
            ));
        }

        if config.cache_ttl_secs > 86400 {
            return Err(RequestRouterError::Configuration(
                "缓存TTL不能超过24小时".to_string()
            ));
        }

        Ok(())
    }

    /// 执行向量搜索请求
    pub async fn execute_vector_search(
        &self,
        search_request: SearchRequest,
        route_info: RequestRouteInfo,
    ) -> Result<RouteExecutionResult<GrpcSearchResponse>, RequestRouterError> {
        // 参数验证
        if search_request.vector.is_empty() {
            return Err(RequestRouterError::Configuration(
                "搜索向量不能为空".to_string()
            ));
        }

        // 检查缓存
        if self.config.enable_request_cache {
            if let Some(cached_response) = self.search_cache.get(&route_info.request_id).await {
                info!("搜索请求 {} 命中缓存", route_info.request_id);
                self.update_cache_metrics(true).await;
                return Ok(RouteExecutionResult {
                    response: cached_response,
                    executed_node: "cache".to_string(),
                    execution_time_ms: 0,
                    used_failover: false,
                    retry_count: 0,
                });
            }
        }

        self.update_cache_metrics(false).await;

        // 执行请求
        let result = self.execute_request_with_routing(
            route_info.clone(),
            |node_id| {
                let search_request = search_request.clone();
                async move {
                    self.network_client
                        .send_search_request_to_router(&node_id, &search_request)
                        .await
                        .map_err(|e| RequestRouterError::Network(VectorDbError::NetworkError(e.to_string())))
                }
            },
        ).await?;

        // 缓存结果
        if self.config.enable_request_cache && result.retry_count == 0 {
            self.search_cache.set(route_info.request_id, result.response.clone()).await;
        }

        Ok(result)
    }

    /// 执行文档插入请求
    pub async fn execute_document_insert(
        &self,
        document: Document,
        route_info: RequestRouteInfo,
    ) -> Result<RouteExecutionResult<String>, RequestRouterError> {
        // 参数验证
        if document.id.trim().is_empty() {
            return Err(RequestRouterError::Configuration(
                "文档ID不能为空".to_string()
            ));
        }

        // 执行请求
        let result = self.execute_request_with_routing(
            route_info.clone(),
            |node_id| {
                let document = document.clone();
                async move {
                    self.network_client
                        .send_insert_request(&node_id, &document)
                        .await
                        .map_err(|e| RequestRouterError::Network(VectorDbError::NetworkError(e.to_string())))
                }
            },
        ).await?;

        // 缓存插入结果（如果成功且没有重试）
        if self.config.enable_request_cache && result.retry_count == 0 {
            self.insert_cache.set(route_info.request_id, result.response.clone()).await;
        }

        Ok(result)
    }

    /// 执行批量文档插入
    pub async fn execute_batch_insert(
        &self,
        documents: Vec<Document>,
        route_info: RequestRouteInfo,
    ) -> Result<RouteExecutionResult<Vec<String>>, RequestRouterError> {
        // 参数验证
        if documents.is_empty() {
            return Err(RequestRouterError::Configuration(
                "文档列表不能为空".to_string()
            ));
        }

        if documents.len() > 1000 {
            return Err(RequestRouterError::Configuration(
                "批量插入文档数量不能超过1000".to_string()
            ));
        }

        for doc in &documents {
            if doc.id.trim().is_empty() {
                return Err(RequestRouterError::Configuration(
                    "批量插入中包含空文档ID".to_string()
                ));
            }
        }

        // 执行请求
        self.execute_request_with_routing(
            route_info,
            |node_id| {
                let documents = documents.clone();
                async move {
                    self.network_client
                        .send_batch_insert_request(&node_id, &documents)
                        .await
                        .map_err(|e| RequestRouterError::Network(VectorDbError::NetworkError(e.to_string())))
                }
            },
        ).await
    }

    /// 通用的带路由的请求执行
    async fn execute_request_with_routing<T, F, Fut>(
        &self,
        route_info: RequestRouteInfo,
        request_fn: F,
    ) -> Result<RouteExecutionResult<T>, RequestRouterError>
    where
        F: Fn(NodeId) -> Fut,
        Fut: std::future::Future<Output = Result<T, RequestRouterError>>,
        T: Clone,
    {
        let start_time = Instant::now();
        let mut retry_count = 0;
        let mut used_failover = false;

        // 获取路由结果
        let route_result = self.load_balancer
            .route_request(&format!("{:?}", route_info.request_type))
            .await?;

        let mut target_nodes = vec![route_result.target_node];
        target_nodes.extend(route_result.backup_nodes);

        // 尝试执行请求
        for (attempt, node_id) in target_nodes.iter().enumerate() {
            if attempt > 0 {
                used_failover = true;
                retry_count += 1;
                warn!("故障转移到节点: {}, 重试次数: {}", node_id, retry_count);

                if retry_count > self.config.max_retries {
                    break;
                }
            }

            // 检查连接池
            self.ensure_connection_pool(node_id).await;

            match tokio::time::timeout(
                Duration::from_millis(self.config.request_timeout_ms),
                request_fn(node_id.clone())
            ).await {
                Ok(Ok(response)) => {
                    let execution_time_ms = start_time.elapsed().as_millis() as u64;
                    
                    // 更新节点性能指标
                    if let Err(e) = self.load_balancer
                        .update_node_health(node_id, true, execution_time_ms as f64)
                        .await {
                        warn!("更新节点健康状态失败: {}", e);
                    }

                    // 更新路由指标
                    self.update_routing_metrics(node_id, execution_time_ms, true, used_failover, retry_count).await;

                    return Ok(RouteExecutionResult {
                        response,
                        executed_node: node_id.clone(),
                        execution_time_ms,
                        used_failover,
                        retry_count,
                    });
                },
                Ok(Err(e)) => {
                    error!("节点 {} 请求执行失败: {}", node_id, e);
                    
                    // 更新节点健康状态
                    if let Err(update_err) = self.load_balancer
                        .update_node_health(node_id, false, 999999.0)
                        .await {
                        warn!("更新节点健康状态失败: {}", update_err);
                    }
                },
                Err(_) => {
                    error!("节点 {} 请求超时", node_id);
                    
                    // 更新节点健康状态
                    if let Err(e) = self.load_balancer
                        .update_node_health(node_id, false, self.config.request_timeout_ms as f64)
                        .await {
                        warn!("更新节点健康状态失败: {}", e);
                    }
                }
            }
        }

        // 所有节点都失败
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        self.update_routing_metrics(&target_nodes[0], execution_time_ms, false, used_failover, retry_count).await;
        
        Err(RequestRouterError::AllNodesUnavailable)
    }

    /// 确保节点有连接池
    async fn ensure_connection_pool(&self, node_id: &NodeId) {
        let mut pools = self.connection_pools.write().await;
        if !pools.contains_key(node_id) {
            let pool = Arc::new(ConnectionPool {
                node_id: node_id.clone(),
                active_connections: 0,
                max_connections: self.config.connection_pool_size,
                last_used: Instant::now(),
            });
            pools.insert(node_id.clone(), pool);
            debug!("为节点 {} 创建连接池", node_id);
        }
    }

    /// 启动后台维护任务
    pub async fn start_background_tasks(&self) -> Result<(), RequestRouterError> {
        // 启动连接池清理任务
        let pools = self.connection_pools.clone();
        let pool_cleanup_interval = Duration::from_secs(300); // 5分钟清理一次
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(pool_cleanup_interval);
            
            loop {
                interval.tick().await;
                
                let mut pools_write = pools.write().await;
                let now = Instant::now();
                
                // 清理超过10分钟未使用的连接池
                pools_write.retain(|node_id, pool| {
                    if now.duration_since(pool.last_used) > Duration::from_secs(600) {
                        debug!("清理节点 {} 的连接池", node_id);
                        false
                    } else {
                        true
                    }
                });
            }
        });

        // 启动缓存清理任务
        if self.config.enable_request_cache {
            let search_cache = self.search_cache.storage.clone();
            let insert_cache = self.insert_cache.storage.clone();
            let cache_cleanup_interval = Duration::from_secs(60); // 每分钟清理一次过期缓存
            
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(cache_cleanup_interval);
                
                loop {
                    interval.tick().await;
                    
                    // 清理搜索缓存
                    {
                        let mut cache_write = search_cache.write().await;
                        let now = Instant::now();
                        cache_write.retain(|_, cached| {
                            now.duration_since(cached.cached_at) < cached.ttl
                        });
                    }
                    
                    // 清理插入缓存
                    {
                        let mut cache_write = insert_cache.write().await;
                        let now = Instant::now();
                        cache_write.retain(|_, cached| {
                            now.duration_since(cached.cached_at) < cached.ttl
                        });
                    }
                }
            });
        }

        info!("请求路由器后台任务已启动");
        Ok(())
    }

    /// 更新缓存指标
    async fn update_cache_metrics(&self, cache_hit: bool) {
        let mut metrics = self.routing_metrics.write().await;
        if cache_hit {
            metrics.cache_hits += 1;
        } else {
            metrics.cache_misses += 1;
        }
    }

    /// 更新路由指标
    async fn update_routing_metrics(
        &self,
        node_id: &NodeId,
        execution_time_ms: u64,
        success: bool,
        used_failover: bool,
        _retry_count: u32,
    ) {
        let mut metrics = self.routing_metrics.write().await;
        
        metrics.total_requests += 1;
        if success {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }

        if used_failover {
            metrics.failover_count += 1;
        }

        // 更新平均响应时间
        let total_response_time = metrics.avg_response_time_ms * (metrics.total_requests - 1) as f64 + execution_time_ms as f64;
        metrics.avg_response_time_ms = total_response_time / metrics.total_requests as f64;

        // 更新节点请求分布
        *metrics.requests_per_node.entry(node_id.clone()).or_insert(0) += 1;
    }

    /// 获取集群健康状态
    pub async fn get_cluster_health(&self) -> ClusterHealthStatus {
        let node_statuses = self.load_balancer.get_all_nodes_status().await;
        let healthy_nodes = node_statuses.values().filter(|n| n.is_healthy).count();
        let total_nodes = node_statuses.len();

        let health_percentage = if total_nodes > 0 {
            (healthy_nodes as f64 / total_nodes as f64) * 100.0
        } else {
            0.0
        };

        ClusterHealthStatus {
            total_nodes,
            healthy_nodes,
            health_percentage,
            node_details: node_statuses.into_iter().map(|(id, weight)| {
                NodeHealthDetail {
                    node_id: id,
                    is_healthy: weight.is_healthy,
                    avg_response_time_ms: weight.avg_response_time_ms,
                    current_connections: weight.current_connections,
                    weight: weight.weight,
                    last_updated_timestamp: weight.last_updated.elapsed().as_secs() as i64,
                }
            }).collect(),
        }
    }

    /// 获取路由性能指标
    pub async fn get_routing_metrics(&self) -> RoutingMetrics {
        self.routing_metrics.read().await.clone()
    }

    /// 获取负载均衡状态
    pub async fn get_load_balance_status(&self) -> LoadBalanceStatus {
        let balance_report = self.load_balancer.check_load_balance().await;

        LoadBalanceStatus {
            is_balanced: balance_report.is_balanced,
            deviation: balance_report.deviation,
            total_requests: balance_report.total_requests,
            node_distributions: balance_report.node_distributions,
            routing_strategy: "智能负载均衡".to_string(),
        }
    }
}

/// 集群健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealthStatus {
    /// 总节点数
    pub total_nodes: usize,
    /// 健康节点数
    pub healthy_nodes: usize,
    /// 健康百分比
    pub health_percentage: f64,
    /// 节点详细信息
    pub node_details: Vec<NodeHealthDetail>,
}

/// 节点健康详细信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthDetail {
    /// 节点ID
    pub node_id: NodeId,
    /// 是否健康
    pub is_healthy: bool,
    /// 平均响应时间
    pub avg_response_time_ms: f64,
    /// 当前连接数
    pub current_connections: u32,
    /// 权重
    pub weight: f64,
    /// 最后更新时间 (timestamp)
    pub last_updated_timestamp: i64,
}

/// 负载均衡状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalanceStatus {
    /// 是否均衡
    pub is_balanced: bool,
    /// 偏差比例
    pub deviation: f64,
    /// 总请求数
    pub total_requests: u64,
    /// 节点分布
    pub node_distributions: HashMap<NodeId, f64>,
    /// 路由策略
    pub routing_strategy: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distributed::load_balancer::{LoadBalancerConfig, IntelligentLoadBalancer};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_request_router_creation() {
        let lb_config = LoadBalancerConfig::default();
        let load_balancer = Arc::new(IntelligentLoadBalancer::new(lb_config));
        let network_client = Arc::new(DistributedNetworkClient::new());
        let routing_config = RoutingConfig::default();

        let router = ClusterAwareRequestRouter::new(
            load_balancer,
            network_client,
            routing_config,
        );

        let health = router.get_cluster_health().await;
        assert_eq!(health.total_nodes, 0);
    }

    #[tokio::test]
    async fn test_routing_metrics() {
        let lb_config = LoadBalancerConfig::default();
        let load_balancer = Arc::new(IntelligentLoadBalancer::new(lb_config));
        let network_client = Arc::new(DistributedNetworkClient::new());
        let routing_config = RoutingConfig::default();

        let router = ClusterAwareRequestRouter::new(
            load_balancer,
            network_client,
            routing_config,
        );

        let metrics = router.get_routing_metrics().await;
        assert_eq!(metrics.total_requests, 0);
    }
}