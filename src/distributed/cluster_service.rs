use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};
use serde::{Deserialize, Serialize};

use crate::distributed::{
    ClusterManager, IntelligentLoadBalancer, LoadBalancerConfig, ClusterAwareRequestRouter,
    RoutingConfig, DistributedNetworkClient, LoadBalancerError
};
use crate::types::*;
use thiserror::Error;

/// 集群服务错误类型
#[derive(Error, Debug)]
pub enum ClusterServiceError {
    #[error("配置错误: {0}")]
    Configuration(String),
    
    #[error("负载均衡器错误: {0}")]
    LoadBalancer(#[from] LoadBalancerError),
    
    #[error("集群管理错误: {0}")]
    ClusterManagement(String),
    
    #[error("网络错误: {0}")]
    Network(String),
    
    #[error("存储错误: {0}")]
    Storage(#[from] VectorDbError),
    
    #[error("服务未启动")]
    ServiceNotStarted,
}

/// 集群服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterServiceConfig {
    /// 集群配置
    pub cluster_config: ClusterConfig,
    /// 负载均衡配置
    pub load_balancer_config: LoadBalancerConfig,
    /// 路由配置
    pub routing_config: RoutingConfig,
    /// 本地节点信息
    pub local_node: NodeInfo,
    /// 是否启用内置负载均衡
    pub enable_builtin_load_balancer: bool,
    /// 服务发现配置
    pub service_discovery_config: ServiceDiscoveryConfig,
}

/// 服务发现配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDiscoveryConfig {
    /// 是否启用自动发现
    pub enable_auto_discovery: bool,
    /// 种子节点列表
    pub seed_nodes: Vec<String>,
    /// 发现间隔 (秒)
    pub discovery_interval_secs: u64,
    /// 节点健康检查间隔 (秒)
    pub health_check_interval_secs: u64,
}

impl Default for ServiceDiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_auto_discovery: true,
            seed_nodes: Vec::new(),
            discovery_interval_secs: 60,
            health_check_interval_secs: 30,
        }
    }
}

impl Default for ClusterServiceConfig {
    fn default() -> Self {
        Self {
            cluster_config: ClusterConfig::default(),
            load_balancer_config: LoadBalancerConfig::default(),
            routing_config: RoutingConfig::default(),
            local_node: NodeInfo::default(),
            enable_builtin_load_balancer: true,
            service_discovery_config: ServiceDiscoveryConfig::default(),
        }
    }
}

/// 集群服务状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterServiceStatus {
    /// 是否正在运行
    pub is_running: bool,
    /// 集群节点数量
    pub total_nodes: usize,
    /// 健康节点数量
    pub healthy_nodes: usize,
    /// 本地节点状态
    pub local_node_status: NodeState,
    /// 负载均衡状态
    pub load_balance_status: Option<crate::distributed::LoadBalanceStatus>,
    /// 最后更新时间
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// 集群服务 - 提供内置负载均衡的完整解决方案
pub struct ClusterService {
    /// 配置
    config: ClusterServiceConfig,
    /// 集群管理器
    cluster_manager: Arc<ClusterManager>,
    /// 负载均衡器
    load_balancer: Option<Arc<IntelligentLoadBalancer>>,
    /// 请求路由器
    request_router: Option<Arc<ClusterAwareRequestRouter>>,
    /// 网络客户端
    network_client: Arc<DistributedNetworkClient>,
    /// 服务状态
    status: Arc<RwLock<ClusterServiceStatus>>,
}

impl ClusterService {
    /// 创建新的集群服务
    pub async fn new(
        config: ClusterServiceConfig,
        storage: Arc<crate::advanced_storage::AdvancedStorage>,
    ) -> Result<Self, ClusterServiceError> {
        info!("正在初始化集群服务...");

        // 配置验证
        Self::validate_config(&config)?;

        // 创建集群管理器
        let cluster_manager = Arc::new(ClusterManager::new(
            config.cluster_config.clone(),
            config.local_node.clone(),
            storage,
        ).map_err(|e| ClusterServiceError::ClusterManagement(e.to_string()))?);

        // 创建网络客户端
        let network_client = Arc::new(DistributedNetworkClient::with_node_id(
            config.local_node.id.clone()
        ));

        // 根据配置创建负载均衡器和路由器
        let (load_balancer, request_router) = if config.enable_builtin_load_balancer {
            let lb = Arc::new(IntelligentLoadBalancer::new(config.load_balancer_config.clone())?);
            let router = Arc::new(ClusterAwareRequestRouter::new(
                lb.clone(),
                network_client.clone(),
                config.routing_config.clone(),
            ).map_err(|e| ClusterServiceError::Configuration(e.to_string()))?);
            (Some(lb), Some(router))
        } else {
            (None, None)
        };

        // 初始化状态
        let status = Arc::new(RwLock::new(ClusterServiceStatus {
            is_running: false,
            total_nodes: 1, // 包含本地节点
            healthy_nodes: 1,
            local_node_status: NodeState::Healthy,
            load_balance_status: None,
            last_updated: chrono::Utc::now(),
        }));

        Ok(Self {
            config,
            cluster_manager,
            load_balancer,
            request_router,
            network_client,
            status,
        })
    }

    /// 验证配置
    fn validate_config(config: &ClusterServiceConfig) -> Result<(), ClusterServiceError> {
        // 验证本地节点信息
        if config.local_node.id.trim().is_empty() {
            return Err(ClusterServiceError::Configuration(
                "本地节点ID不能为空".to_string()
            ));
        }

        if config.local_node.address.trim().is_empty() {
            return Err(ClusterServiceError::Configuration(
                "本地节点地址不能为空".to_string()
            ));
        }

        if config.local_node.port == 0 || config.local_node.port > 65535 {
            return Err(ClusterServiceError::Configuration(
                format!("本地节点端口无效: {}", config.local_node.port)
            ));
        }

        // 验证服务发现配置
        if config.service_discovery_config.enable_auto_discovery {
            for seed_node in &config.service_discovery_config.seed_nodes {
                if !Self::is_valid_node_address(seed_node) {
                    return Err(ClusterServiceError::Configuration(
                        format!("无效的种子节点地址: {}", seed_node)
                    ));
                }
            }

            if config.service_discovery_config.discovery_interval_secs == 0 
                || config.service_discovery_config.discovery_interval_secs > 3600 {
                return Err(ClusterServiceError::Configuration(
                    "发现间隔必须在1秒-1小时范围内".to_string()
                ));
            }
        }

        Ok(())
    }

    /// 启动集群服务
    pub async fn start(&self) -> Result<(), ClusterServiceError> {
        info!("启动集群服务...");

        // 启动集群管理器
        self.cluster_manager.start().await
            .map_err(|e| ClusterServiceError::ClusterManagement(e.to_string()))?;

        // 启动负载均衡器后台服务
        if let Some(ref load_balancer) = self.load_balancer {
            load_balancer.start_background_services().await
                .map_err(|e| ClusterServiceError::LoadBalancer(
                    crate::distributed::LoadBalancerError::InternalError(e.to_string())
                ))?;
            
            // 将本地节点添加到负载均衡器
            load_balancer.add_node(self.config.local_node.id.clone(), 1.0).await?;
        }

        // 启动请求路由器后台服务
        if let Some(ref request_router) = self.request_router {
            request_router.start_background_tasks().await
                .map_err(|e| ClusterServiceError::Configuration(e.to_string()))?;
        }

        // 启动服务发现
        if self.config.service_discovery_config.enable_auto_discovery {
            self.start_service_discovery().await
                .map_err(|e| ClusterServiceError::Network(e.to_string()))?;
        }

        // 更新状态
        {
            let mut status = self.status.write().await;
            status.is_running = true;
            status.last_updated = chrono::Utc::now();
        }

        info!("集群服务启动完成");
        Ok(())
    }

    /// 停止集群服务
    pub async fn stop(&self) -> Result<(), ClusterServiceError> {
        info!("停止集群服务...");

        // 从集群中优雅退出
        self.cluster_manager.leave_cluster(false).await
            .map_err(|e| ClusterServiceError::ClusterManagement(e.to_string()))?;

        // 更新状态
        {
            let mut status = self.status.write().await;
            status.is_running = false;
            status.last_updated = chrono::Utc::now();
        }

        info!("集群服务已停止");
        Ok(())
    }

    /// 获取请求路由器 (主要API)
    pub fn get_request_router(&self) -> Option<&Arc<ClusterAwareRequestRouter>> {
        self.request_router.as_ref()
    }

    /// 获取负载均衡器
    pub fn get_load_balancer(&self) -> Option<&Arc<IntelligentLoadBalancer>> {
        self.load_balancer.as_ref()
    }

    /// 获取集群管理器
    pub fn get_cluster_manager(&self) -> &Arc<ClusterManager> {
        &self.cluster_manager
    }

    /// 添加节点到集群
    pub async fn add_node(&self, node: NodeInfo) -> Result<(), ClusterServiceError> {
        // 参数验证
        if node.id.trim().is_empty() {
            return Err(ClusterServiceError::Configuration(
                "节点ID不能为空".to_string()
            ));
        }

        if node.address.trim().is_empty() {
            return Err(ClusterServiceError::Configuration(
                "节点地址不能为空".to_string()
            ));
        }

        if node.port == 0 || node.port > 65535 {
            return Err(ClusterServiceError::Configuration(
                format!("节点端口无效: {}", node.port)
            ));
        }

        // 添加到集群管理器
        self.cluster_manager.add_node(node.clone()).await
            .map_err(|e| ClusterServiceError::ClusterManagement(e.to_string()))?;

        // 添加到负载均衡器
        if let Some(ref load_balancer) = self.load_balancer {
            load_balancer.add_node(node.id.clone(), 1.0).await?;
        }

        // 更新状态
        self.update_cluster_status().await;

        info!("节点 {} 已添加到集群", node.id);
        Ok(())
    }

    /// 移除节点
    pub async fn remove_node(&self, node_id: &NodeId) -> Result<(), ClusterServiceError> {
        // 参数验证
        if node_id.trim().is_empty() {
            return Err(ClusterServiceError::Configuration(
                "节点ID不能为空".to_string()
            ));
        }

        // 从集群管理器移除
        self.cluster_manager.remove_node(node_id).await
            .map_err(|e| ClusterServiceError::ClusterManagement(e.to_string()))?;

        // 从负载均衡器移除
        if let Some(ref load_balancer) = self.load_balancer {
            load_balancer.remove_node(node_id).await;
        }

        // 更新状态
        self.update_cluster_status().await;

        info!("节点 {} 已从集群移除", node_id);
        Ok(())
    }

    /// 获取集群状态
    pub async fn get_cluster_status(&self) -> ClusterServiceStatus {
        self.status.read().await.clone()
    }

    /// 获取详细的集群健康状态
    pub async fn get_cluster_health(&self) -> Result<crate::distributed::ClusterHealthStatus, ClusterServiceError> {
        if let Some(ref request_router) = self.request_router {
            Ok(request_router.get_cluster_health().await)
        } else {
            // 如果没有启用内置负载均衡，返回基本状态
            Ok(crate::distributed::ClusterHealthStatus {
                total_nodes: 1,
                healthy_nodes: 1,
                health_percentage: 100.0,
                node_details: vec![crate::distributed::NodeHealthDetail {
                    node_id: self.config.local_node.id.clone(),
                    is_healthy: true,
                    avg_response_time_ms: 0.0,
                    current_connections: 0,
                    weight: 1.0,
                    last_updated_timestamp: chrono::Utc::now().timestamp(),
                }],
            })
        }
    }

    /// 获取负载均衡状态
    pub async fn get_load_balance_status(&self) -> Option<crate::distributed::LoadBalanceStatus> {
        if let Some(ref request_router) = self.request_router {
            Some(request_router.get_load_balance_status().await)
        } else {
            None
        }
    }

    /// 是否启用了内置负载均衡
    pub fn is_builtin_load_balancer_enabled(&self) -> bool {
        self.config.enable_builtin_load_balancer
    }

    /// 启动服务发现
    async fn start_service_discovery(&self) -> Result<(), ClusterServiceError> {
        if self.config.service_discovery_config.seed_nodes.is_empty() {
            warn!("未配置种子节点，跳过服务发现");
            return Ok(());
        }

        let config = self.config.service_discovery_config.clone();
        let network_client = self.network_client.clone();
        let load_balancer = self.load_balancer.clone();
        let cluster_manager = self.cluster_manager.clone();

        tokio::spawn(async move {
            let mut discovery_interval = tokio::time::interval(Duration::from_secs(config.discovery_interval_secs));
            let mut consecutive_failures = HashMap::new();
            
            loop {
                discovery_interval.tick().await;
                
                for seed_node in &config.seed_nodes {
                    // 验证种子节点格式
                    if !Self::is_valid_node_address(seed_node) {
                        warn!("无效的种子节点地址格式: {}", seed_node);
                        continue;
                    }

                    // 尝试连接种子节点并发现其他节点
                    match network_client.send_health_check(&seed_node.clone(), seed_node).await {
                        Ok(_) => {
                            info!("发现健康的种子节点: {}", seed_node);
                            
                            // 重置连续失败计数
                            consecutive_failures.insert(seed_node.clone(), 0);
                            
                            // 如果有负载均衡器，添加到负载均衡器
                            if let Some(ref lb) = load_balancer {
                                if let Err(e) = lb.add_node(seed_node.clone(), 1.0).await {
                                    debug!("节点已存在于负载均衡器: {} - {}", seed_node, e);
                                }
                            }
                            
                            // 尝试加入集群
                            if let Err(e) = cluster_manager.join_cluster(seed_node).await {
                                warn!("加入集群失败，种子节点: {}, 错误: {}", seed_node, e);
                            }
                        },
                        Err(e) => {
                            let failure_count = consecutive_failures.entry(seed_node.clone()).or_insert(0);
                            *failure_count += 1;
                            
                            if *failure_count <= 3 {
                                warn!("种子节点不可达: {}, 连续失败 {} 次, 错误: {}", seed_node, failure_count, e);
                            } else {
                                debug!("种子节点持续不可达: {}, 连续失败 {} 次", seed_node, failure_count);
                            }
                            
                            // 如果连续失败超过阈值，从负载均衡器移除
                            if *failure_count > 5 {
                                if let Some(ref lb) = load_balancer {
                                    lb.remove_node(seed_node).await;
                                }
                            }
                        }
                    }
                }
            }
        });

        info!("服务发现已启动，种子节点: {:?}", config.seed_nodes);
        Ok(())
    }

    /// 验证节点地址格式
    fn is_valid_node_address(address: &str) -> bool {
        // 基本格式验证：host:port
        if let Some(colon_pos) = address.rfind(':') {
            let host = &address[..colon_pos];
            let port_str = &address[colon_pos + 1..];
            
            // 检查主机名不为空
            if host.is_empty() {
                return false;
            }
            
            // 检查端口是否有效
            if let Ok(port) = port_str.parse::<u16>() {
                return port > 0 && port < 65536;
            }
        }
        false
    }

    /// 更新集群状态
    async fn update_cluster_status(&self) {
        if let Some(ref request_router) = self.request_router {
            if let Ok(health_status) = request_router.get_cluster_health().await {
                let mut status = self.status.write().await;
                status.total_nodes = health_status.total_nodes;
                status.healthy_nodes = health_status.healthy_nodes;
                status.load_balance_status = Some(request_router.get_load_balance_status().await);
                status.last_updated = chrono::Utc::now();
            }
        }
    }

    /// 检查集群是否健康
    pub async fn is_cluster_healthy(&self) -> bool {
        if let Ok(health) = self.get_cluster_health().await {
            health.health_percentage >= 50.0 // 至少50%的节点健康
        } else {
            true // 如果无法获取状态，假设健康
        }
    }

    /// 获取推荐的API使用方式
    pub fn get_api_usage_guide(&self) -> String {
        if self.config.enable_builtin_load_balancer {
            format!(
                "集群服务已启用内置负载均衡。\n\
                客户端可以连接到任何节点: \n\
                - 本地节点: {}:{}\n\
                - 所有请求将被智能路由到最佳节点\n\
                - 支持自动故障转移和负载均衡\n\
                - 无需外部 nginx 或 HAProxy",
                self.config.local_node.address,
                self.config.local_node.port
            )
        } else {
            "集群服务未启用内置负载均衡，建议使用外部负载均衡器 (如 nginx)".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cluster_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(
            crate::advanced_storage::AdvancedStorage::new(temp_dir.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let config = ClusterServiceConfig::default();
        let service = ClusterService::new(config, storage).await.unwrap();

        assert!(service.is_builtin_load_balancer_enabled());
        assert!(service.get_request_router().is_some());
        assert!(service.get_load_balancer().is_some());
    }

    #[tokio::test]
    async fn test_cluster_service_without_load_balancer() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(
            crate::advanced_storage::AdvancedStorage::new(temp_dir.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let mut config = ClusterServiceConfig::default();
        config.enable_builtin_load_balancer = false;

        let service = ClusterService::new(config, storage).await.unwrap();

        assert!(!service.is_builtin_load_balancer_enabled());
        assert!(service.get_request_router().is_none());
        assert!(service.get_load_balancer().is_none());
    }

    #[tokio::test]
    async fn test_api_usage_guide() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(
            crate::advanced_storage::AdvancedStorage::new(temp_dir.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let config = ClusterServiceConfig::default();
        let service = ClusterService::new(config, storage).await.unwrap();

        let guide = service.get_api_usage_guide();
        assert!(guide.contains("内置负载均衡"));
        assert!(guide.contains("无需外部 nginx"));
    }
}