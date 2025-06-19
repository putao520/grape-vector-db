use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

use crate::distributed::load_balancer::{IntelligentLoadBalancer, RouteResult};
use crate::distributed::network_client::DistributedNetworkClient;
use crate::types::*;

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
    /// 请求缓存
    request_cache: Arc<RwLock<HashMap<String, CachedResponse>>>,
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
pub struct CachedResponse {
    /// 响应数据
    pub data: Vec<u8>,
    /// 缓存时间
    pub cached_at: Instant,
    /// TTL
    pub ttl: Duration,
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
    ) -> Self {
        Self {
            load_balancer,
            network_client,
            config,
            connection_pools: Arc::new(RwLock::new(HashMap::new())),
            request_cache: Arc::new(RwLock::new(HashMap::new())),
            routing_metrics: Arc::new(RwLock::new(RoutingMetrics::default())),
        }
    }

    /// 执行向量搜索请求
    pub async fn execute_vector_search(
        &self,
        search_request: SearchRequest,
        route_info: RequestRouteInfo,
    ) -> Result<RouteExecutionResult<GrpcSearchResponse>, VectorDbError> {
        self.execute_request_with_routing(
            route_info,
            |node_id| async move {
                self.network_client
                    .send_search_request(&node_id, &search_request)
                    .await
                    .map_err(|e| VectorDbError::NetworkError(e.to_string()))
            },
        ).await
    }

    /// 执行文档插入请求
    pub async fn execute_document_insert(
        &self,
        document: Document,
        route_info: RequestRouteInfo,
    ) -> Result<RouteExecutionResult<String>, VectorDbError> {
        self.execute_request_with_routing(
            route_info,
            |node_id| async move {
                self.network_client
                    .send_insert_request(&node_id, &document)
                    .await
                    .map_err(|e| VectorDbError::NetworkError(e.to_string()))
            },
        ).await
    }

    /// 执行批量文档插入
    pub async fn execute_batch_insert(
        &self,
        documents: Vec<Document>,
        route_info: RequestRouteInfo,
    ) -> Result<RouteExecutionResult<Vec<String>>, VectorDbError> {
        self.execute_request_with_routing(
            route_info,
            |node_id| async move {
                self.network_client
                    .send_batch_insert_request(&node_id, &documents)
                    .await
                    .map_err(|e| VectorDbError::NetworkError(e.to_string()))
            },
        ).await
    }

    /// 通用的带路由的请求执行
    async fn execute_request_with_routing<T, F, Fut>(
        &self,
        route_info: RequestRouteInfo,
        request_fn: F,
    ) -> Result<RouteExecutionResult<T>, VectorDbError>
    where
        F: Fn(NodeId) -> Fut,
        Fut: std::future::Future<Output = Result<T, VectorDbError>>,
    {
        let start_time = Instant::now();
        let mut retry_count = 0;
        let mut used_failover = false;

        // 检查缓存
        if self.config.enable_request_cache {
            if let Some(cached) = self.check_cache(&route_info.request_id).await {
                // 如果支持类型转换，返回缓存结果
                info!("请求 {} 命中缓存", route_info.request_id);
                self.update_cache_metrics(true).await;
                // 注意：这里简化处理，实际实现需要处理类型转换
            }
        }

        self.update_cache_metrics(false).await;

        // 获取路由结果
        let route_result = self.load_balancer
            .route_request(&format!("{:?}", route_info.request_type))
            .await
            .ok_or_else(|| VectorDbError::NetworkError("没有可用的健康节点".to_string()))?;

        let mut target_nodes = vec![route_result.target_node];
        target_nodes.extend(route_result.backup_nodes);

        // 尝试执行请求
        for (attempt, node_id) in target_nodes.iter().enumerate() {
            if attempt > 0 {
                used_failover = true;
                retry_count += 1;
                warn!("故障转移到节点: {}, 重试次数: {}", node_id, retry_count);
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
                    self.load_balancer
                        .update_node_health(node_id, true, execution_time_ms as f64)
                        .await;

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
                    self.load_balancer
                        .update_node_health(node_id, false, 999999.0)
                        .await;

                    if retry_count >= self.config.max_retries {
                        break;
                    }
                },
                Err(_) => {
                    error!("节点 {} 请求超时", node_id);
                    
                    // 更新节点健康状态
                    self.load_balancer
                        .update_node_health(node_id, false, self.config.request_timeout_ms as f64)
                        .await;

                    if retry_count >= self.config.max_retries {
                        break;
                    }
                }
            }
        }

        // 所有节点都失败
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        self.update_routing_metrics(&target_nodes[0], execution_time_ms, false, used_failover, retry_count).await;
        
        Err(VectorDbError::NetworkError(
            "所有节点都不可用".to_string()
        ))
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

    /// 检查请求缓存
    async fn check_cache(&self, request_id: &str) -> Option<CachedResponse> {
        let cache = self.request_cache.read().await;
        if let Some(cached) = cache.get(request_id) {
            if cached.cached_at.elapsed() < cached.ttl {
                return Some(cached.clone());
            }
        }
        None
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
        retry_count: u32,
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
                    last_updated: weight.last_updated,
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
        let routing_stats = self.load_balancer.get_routing_stats().await;

        LoadBalanceStatus {
            is_balanced: balance_report.is_balanced,
            deviation: balance_report.deviation,
            total_requests: balance_report.total_requests,
            node_distributions: balance_report.node_distributions,
            routing_strategy: "智能负载均衡".to_string(),
        }
    }

    /// 启动后台维护任务
    pub async fn start_background_tasks(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            let cache = self.request_cache.clone();
            let cache_cleanup_interval = Duration::from_secs(60); // 每分钟清理一次过期缓存
            
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(cache_cleanup_interval);
                
                loop {
                    interval.tick().await;
                    
                    let mut cache_write = cache.write().await;
                    let now = Instant::now();
                    
                    cache_write.retain(|_, cached| {
                        now.duration_since(cached.cached_at) < cached.ttl
                    });
                }
            });
        }

        info!("请求路由器后台任务已启动");
        Ok(())
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
    /// 最后更新时间
    #[serde(skip)]
    pub last_updated: Instant,
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