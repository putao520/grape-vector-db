use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

use crate::types::*;
use thiserror::Error;

/// 负载均衡器错误类型
#[derive(Error, Debug)]
pub enum LoadBalancerError {
    #[error("无效参数: {0}")]
    InvalidParameter(String),
    
    #[error("节点已存在: {0}")]
    NodeAlreadyExists(NodeId),
    
    #[error("节点不存在: {0}")]
    NodeNotFound(NodeId),
    
    #[error("没有可用的健康节点")]
    NoHealthyNodes,
    
    #[error("负载均衡器配置错误: {0}")]
    ConfigurationError(String),
    
    #[error("内部错误: {0}")]
    InternalError(String),
}

/// 负载均衡策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    /// 轮询
    RoundRobin,
    /// 加权轮询
    WeightedRoundRobin,
    /// 最少连接
    LeastConnections,
    /// 基于负载
    LoadBased,
    /// 地理位置感知
    LocationAware,
}

/// 节点权重信息
#[derive(Debug, Clone)]
pub struct NodeWeight {
    /// 节点ID
    pub node_id: NodeId,
    /// 权重值 (0.0-1.0)
    pub weight: f64,
    /// 当前连接数
    pub current_connections: u32,
    /// 平均响应时间 (毫秒)
    pub avg_response_time_ms: f64,
    /// 健康状态
    pub is_healthy: bool,
    /// 最后更新时间
    pub last_updated: Instant,
    /// 地理位置信息 (可选)
    pub location: Option<NodeLocation>,
}

/// 节点地理位置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLocation {
    /// 数据中心名称
    pub datacenter: String,
    /// 区域名称
    pub region: String,
    /// 可用区
    pub availability_zone: String,
    /// 延迟等级 (1-5, 1最优)
    pub latency_tier: u8,
}

/// 负载均衡器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    /// 负载均衡策略
    pub strategy: LoadBalancingStrategy,
    /// 健康检查间隔
    pub health_check_interval_secs: u64,
    /// 节点不健康阈值 (毫秒)
    pub unhealthy_threshold_ms: u64,
    /// 权重更新间隔
    pub weight_update_interval_secs: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 故障转移超时
    pub failover_timeout_ms: u64,
}

impl Default for LoadBalancerConfig {
    fn default() -> Self {
        Self {
            strategy: LoadBalancingStrategy::LoadBased,
            health_check_interval_secs: 30,
            unhealthy_threshold_ms: 5000,
            weight_update_interval_secs: 60,
            max_retries: 3,
            failover_timeout_ms: 1000,
        }
    }
}

/// 请求路由结果
#[derive(Debug, Clone)]
pub struct RouteResult {
    /// 目标节点ID
    pub target_node: NodeId,
    /// 备选节点列表
    pub backup_nodes: Vec<NodeId>,
    /// 路由决策原因
    pub routing_reason: String,
}

/// 智能负载均衡器
pub struct IntelligentLoadBalancer {
    /// 配置
    config: LoadBalancerConfig,
    /// 节点权重映射
    node_weights: Arc<RwLock<HashMap<NodeId, NodeWeight>>>,
    /// 轮询计数器
    round_robin_counter: Arc<RwLock<usize>>,
    /// 路由统计
    routing_stats: Arc<RwLock<HashMap<NodeId, u64>>>,
}

impl IntelligentLoadBalancer {
    /// 创建新的负载均衡器
    pub fn new(config: LoadBalancerConfig) -> Result<Self, LoadBalancerError> {
        // 配置验证
        Self::validate_config(&config)?;
        
        Ok(Self {
            config,
            node_weights: Arc::new(RwLock::new(HashMap::new())),
            round_robin_counter: Arc::new(RwLock::new(0)),
            routing_stats: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 验证配置
    fn validate_config(config: &LoadBalancerConfig) -> Result<(), LoadBalancerError> {
        if config.health_check_interval_secs == 0 {
            return Err(LoadBalancerError::ConfigurationError(
                "健康检查间隔不能为0".to_string()
            ));
        }

        if config.health_check_interval_secs > 3600 {
            return Err(LoadBalancerError::ConfigurationError(
                "健康检查间隔不能超过1小时".to_string()
            ));
        }

        if config.unhealthy_threshold_ms > 60_000 {
            return Err(LoadBalancerError::ConfigurationError(
                "不健康阈值不能超过60秒".to_string()
            ));
        }

        if config.max_retries > 10 {
            return Err(LoadBalancerError::ConfigurationError(
                "最大重试次数不能超过10次".to_string()
            ));
        }

        if config.failover_timeout_ms > 30_000 {
            return Err(LoadBalancerError::ConfigurationError(
                "故障转移超时不能超过30秒".to_string()
            ));
        }

        Ok(())
    }

    /// 添加节点
    pub async fn add_node(&self, node_id: NodeId, initial_weight: f64) -> Result<(), LoadBalancerError> {
        self.add_node_with_location(node_id, initial_weight, None).await
    }

    /// 添加带位置信息的节点
    pub async fn add_node_with_location(&self, node_id: NodeId, initial_weight: f64, location: Option<NodeLocation>) -> Result<(), LoadBalancerError> {
        // 参数验证
        if node_id.trim().is_empty() {
            return Err(LoadBalancerError::InvalidParameter(
                "节点ID不能为空".to_string()
            ));
        }

        if !(0.0..=1.0).contains(&initial_weight) {
            return Err(LoadBalancerError::InvalidParameter(
                format!("权重必须在0.0-1.0范围内，当前值: {}", initial_weight)
            ));
        }

        // 验证位置信息
        if let Some(ref loc) = location {
            if loc.datacenter.trim().is_empty() || loc.region.trim().is_empty() {
                return Err(LoadBalancerError::InvalidParameter(
                    "数据中心和区域名称不能为空".to_string()
                ));
            }
            if !(1..=5).contains(&loc.latency_tier) {
                return Err(LoadBalancerError::InvalidParameter(
                    format!("延迟等级必须在1-5范围内，当前值: {}", loc.latency_tier)
                ));
            }
        }

        let weight = NodeWeight {
            node_id: node_id.clone(),
            weight: initial_weight,
            current_connections: 0,
            avg_response_time_ms: 50.0,
            is_healthy: true,
            last_updated: Instant::now(),
            location,
        };

        let mut weights = self.node_weights.write().await;
        
        // 检查节点是否已存在
        if weights.contains_key(&node_id) {
            return Err(LoadBalancerError::NodeAlreadyExists(node_id));
        }
        
        weights.insert(node_id.clone(), weight);
        
        info!("添加节点到负载均衡器: {}, 权重: {:.2}, 位置: {:?}", 
              node_id, initial_weight, location);
        
        Ok(())
    }

    /// 移除节点
    pub async fn remove_node(&self, node_id: &NodeId) {
        let mut weights = self.node_weights.write().await;
        weights.remove(node_id);
        
        info!("从负载均衡器移除节点: {}", node_id);
    }

    /// 更新节点健康状态
    pub async fn update_node_health(&self, node_id: &NodeId, is_healthy: bool, response_time_ms: f64) -> Result<(), LoadBalancerError> {
        // 参数验证
        if node_id.trim().is_empty() {
            return Err(LoadBalancerError::InvalidParameter(
                "节点ID不能为空".to_string()
            ));
        }

        if response_time_ms < 0.0 || response_time_ms > 300_000.0 { // 最大5分钟
            return Err(LoadBalancerError::InvalidParameter(
                format!("响应时间无效，必须在0-300000ms范围内: {}", response_time_ms)
            ));
        }

        let mut weights = self.node_weights.write().await;
        let weight = weights.get_mut(node_id)
            .ok_or_else(|| LoadBalancerError::NodeNotFound(node_id.clone()))?;
            
        weight.is_healthy = is_healthy;
        weight.avg_response_time_ms = response_time_ms;
        weight.last_updated = Instant::now();
        
        // 根据响应时间调整权重
        if is_healthy {
            // 响应时间越短，权重越高
            let base_weight = 1.0;
            let time_factor = (1000.0 / (response_time_ms + 100.0)).min(2.0);
            weight.weight = (base_weight * time_factor).clamp(0.1, 1.0);
        } else {
            weight.weight = 0.0; // 不健康节点权重为0
        }

        debug!("更新节点 {} 健康状态: {}, 响应时间: {:.2}ms, 权重: {:.2}", 
               node_id, is_healthy, response_time_ms, weight.weight);
        
        Ok(())
    }

    /// 更新节点连接数
    pub async fn update_node_connections(&self, node_id: &NodeId, connections: u32) {
        let mut weights = self.node_weights.write().await;
        if let Some(weight) = weights.get_mut(node_id) {
            weight.current_connections = connections;
            weight.last_updated = Instant::now();
        }
    }

    /// 选择目标节点进行请求路由
    pub async fn route_request(&self, request_type: &str) -> Result<RouteResult, LoadBalancerError> {
        // 参数验证
        if request_type.trim().is_empty() {
            return Err(LoadBalancerError::InvalidParameter(
                "请求类型不能为空".to_string()
            ));
        }

        let weights = self.node_weights.read().await;
        let healthy_nodes: Vec<_> = weights
            .values()
            .filter(|w| w.is_healthy && w.weight > 0.0)
            .collect();

        if healthy_nodes.is_empty() {
            warn!("没有健康的节点可用于请求路由");
            return Err(LoadBalancerError::NoHealthyNodes);
        }

        let target_node = match self.config.strategy {
            LoadBalancingStrategy::RoundRobin => {
                self.select_round_robin(&healthy_nodes).await
            },
            LoadBalancingStrategy::WeightedRoundRobin => {
                self.select_weighted_round_robin(&healthy_nodes).await
            },
            LoadBalancingStrategy::LeastConnections => {
                self.select_least_connections(&healthy_nodes).await
            },
            LoadBalancingStrategy::LoadBased => {
                self.select_load_based(&healthy_nodes).await
            },
            LoadBalancingStrategy::LocationAware => {
                self.select_location_aware(&healthy_nodes).await
            },
        };

        let target = target_node.ok_or(LoadBalancerError::NoHealthyNodes)?;
        
        // 选择备选节点
        let backup_nodes: Vec<NodeId> = healthy_nodes
            .iter()
            .filter(|w| w.node_id != target)
            .take(2) // 最多2个备选节点
            .map(|w| w.node_id.clone())
            .collect();

        // 更新路由统计
        let mut stats = self.routing_stats.write().await;
        *stats.entry(target.clone()).or_insert(0) += 1;

        Ok(RouteResult {
            target_node: target.clone(),
            backup_nodes,
            routing_reason: format!("使用 {:?} 策略选择节点 {}", self.config.strategy, target),
        })
    }

    /// 轮询选择
    async fn select_round_robin(&self, healthy_nodes: &[&NodeWeight]) -> Option<NodeId> {
        if healthy_nodes.is_empty() {
            return None;
        }

        let mut counter = self.round_robin_counter.write().await;
        let index = *counter % healthy_nodes.len();
        *counter = (*counter + 1) % healthy_nodes.len();
        
        Some(healthy_nodes[index].node_id.clone())
    }

    /// 加权轮询选择
    async fn select_weighted_round_robin(&self, healthy_nodes: &[&NodeWeight]) -> Option<NodeId> {
        if healthy_nodes.is_empty() {
            return None;
        }

        // 计算总权重
        let total_weight: f64 = healthy_nodes.iter().map(|w| w.weight).sum();
        if total_weight <= 0.0 {
            return self.select_round_robin(healthy_nodes).await;
        }

        // 使用随机数进行加权选择
        use fastrand;
        let mut random_weight = fastrand::f64() * total_weight;
        
        for node in healthy_nodes {
            random_weight -= node.weight;
            if random_weight <= 0.0 {
                return Some(node.node_id.clone());
            }
        }

        // 回退到第一个节点
        Some(healthy_nodes[0].node_id.clone())
    }

    /// 最少连接选择
    async fn select_least_connections(&self, healthy_nodes: &[&NodeWeight]) -> Option<NodeId> {
        if healthy_nodes.is_empty() {
            return None;
        }

        let min_connections_node = healthy_nodes
            .iter()
            .min_by_key(|w| w.current_connections)?;

        Some(min_connections_node.node_id.clone())
    }

    /// 基于负载的选择
    async fn select_load_based(&self, healthy_nodes: &[&NodeWeight]) -> Option<NodeId> {
        if healthy_nodes.is_empty() {
            return None;
        }

        // 综合考虑权重、连接数和响应时间
        let best_node = healthy_nodes
            .iter()
            .max_by(|a, b| {
                let score_a = self.calculate_node_score(a);
                let score_b = self.calculate_node_score(b);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })?;

        Some(best_node.node_id.clone())
    }

    /// 基于位置的选择
    async fn select_location_aware(&self, healthy_nodes: &[&NodeWeight]) -> Option<NodeId> {
        if healthy_nodes.is_empty() {
            return None;
        }

        // 按位置优先级分组
        let mut datacenter_groups: HashMap<String, Vec<&NodeWeight>> = HashMap::new();
        let mut no_location_nodes = Vec::new();

        for node in healthy_nodes {
            if let Some(ref location) = node.location {
                datacenter_groups
                    .entry(location.datacenter.clone())
                    .or_insert_with(Vec::new)
                    .push(node);
            } else {
                no_location_nodes.push(*node);
            }
        }

        // 优先选择最优数据中心的节点
        if let Some(best_dc_nodes) = datacenter_groups
            .values()
            .min_by_key(|nodes| {
                nodes.iter()
                    .filter_map(|n| n.location.as_ref())
                    .map(|l| l.latency_tier)
                    .min()
                    .unwrap_or(5)
            }) {
            
            // 在最优数据中心内使用基于负载的选择
            return self.select_load_based(best_dc_nodes).await;
        }

        // 如果没有位置信息，回退到基于负载的选择
        if !no_location_nodes.is_empty() {
            return self.select_load_based(&no_location_nodes).await;
        }

        None
    }
    fn calculate_node_score(&self, node: &NodeWeight) -> f64 {
        if !node.is_healthy {
            return 0.0;
        }

        // 权重占50%，连接数占30%，响应时间占20%
        let weight_score = node.weight;
        let connection_score = 1.0 / (1.0 + node.current_connections as f64 * 0.1);
        let response_time_score = 1000.0 / (node.avg_response_time_ms + 100.0);

        weight_score * 0.5 + connection_score * 0.3 + response_time_score * 0.2
    }

    /// 获取所有节点状态
    pub async fn get_all_nodes_status(&self) -> HashMap<NodeId, NodeWeight> {
        self.node_weights.read().await.clone()
    }

    /// 获取路由统计
    pub async fn get_routing_stats(&self) -> HashMap<NodeId, u64> {
        self.routing_stats.read().await.clone()
    }

    /// 检查负载均衡情况
    pub async fn check_load_balance(&self) -> LoadBalanceReport {
        let stats = self.routing_stats.read().await;
        let weights = self.node_weights.read().await;

        let total_requests: u64 = stats.values().sum();
        let node_count = weights.len();

        if node_count == 0 || total_requests == 0 {
            return LoadBalanceReport {
                is_balanced: true,
                deviation: 0.0,
                total_requests,
                node_distributions: HashMap::new(),
            };
        }

        let expected_per_node = total_requests as f64 / node_count as f64;
        let mut max_deviation = 0.0;
        let mut distributions = HashMap::new();

        for (node_id, &count) in stats.iter() {
            let percentage = count as f64 / total_requests as f64 * 100.0;
            distributions.insert(node_id.clone(), percentage);

            let deviation = (count as f64 - expected_per_node).abs() / expected_per_node;
            max_deviation = max_deviation.max(deviation);
        }

        LoadBalanceReport {
            is_balanced: max_deviation < 0.15, // 15%偏差内认为是均衡的
            deviation: max_deviation,
            total_requests,
            node_distributions: distributions,
        }
    }

    /// 启动负载均衡器后台服务
    pub async fn start_background_services(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config = self.config.clone();
        let weights = self.node_weights.clone();

        // 健康检查服务
        let health_check_weights = weights.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config.health_check_interval_secs));
            
            loop {
                interval.tick().await;
                
                let weights_read = health_check_weights.read().await;
                let unhealthy_nodes: Vec<_> = weights_read
                    .values()
                    .filter(|w| !w.is_healthy || 
                           w.last_updated.elapsed() > Duration::from_secs(config.health_check_interval_secs * 2))
                    .map(|w| w.node_id.clone())
                    .collect();
                drop(weights_read);

                if !unhealthy_nodes.is_empty() {
                    warn!("检测到不健康节点: {:?}", unhealthy_nodes);
                    
                    // 标记长时间未更新的节点为不健康
                    let mut weights_write = health_check_weights.write().await;
                    for node_id in unhealthy_nodes {
                        if let Some(weight) = weights_write.get_mut(&node_id) {
                            if weight.last_updated.elapsed() > Duration::from_secs(config.health_check_interval_secs * 2) {
                                weight.is_healthy = false;
                                weight.weight = 0.0;
                            }
                        }
                    }
                }
            }
        });

        info!("负载均衡器后台服务已启动");
        Ok(())
    }
}

/// 负载均衡报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalanceReport {
    /// 是否均衡
    pub is_balanced: bool,
    /// 最大偏差比例
    pub deviation: f64,
    /// 总请求数
    pub total_requests: u64,
    /// 各节点请求分布百分比
    pub node_distributions: HashMap<NodeId, f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_load_balancer_creation() {
        let config = LoadBalancerConfig::default();
        let lb = IntelligentLoadBalancer::new(config);
        
        // 添加一些测试节点
        lb.add_node("node1".to_string(), 1.0).await;
        lb.add_node("node2".to_string(), 0.8).await;
        lb.add_node("node3".to_string(), 0.6).await;

        let nodes = lb.get_all_nodes_status().await;
        assert_eq!(nodes.len(), 3);
    }

    #[tokio::test]
    async fn test_round_robin_routing() {
        let mut config = LoadBalancerConfig::default();
        config.strategy = LoadBalancingStrategy::RoundRobin;
        let lb = IntelligentLoadBalancer::new(config);
        
        lb.add_node("node1".to_string(), 1.0).await;
        lb.add_node("node2".to_string(), 1.0).await;

        let mut node_counts = HashMap::new();
        
        // 发送多个请求
        for _ in 0..10 {
            if let Some(result) = lb.route_request("test").await {
                *node_counts.entry(result.target_node).or_insert(0) += 1;
            }
        }

        // 验证负载均衡
        assert_eq!(node_counts.len(), 2);
        let values: Vec<_> = node_counts.values().collect();
        assert_eq!(*values[0], 5);
        assert_eq!(*values[1], 5);
    }

    #[tokio::test]
    async fn test_health_based_routing() {
        let mut config = LoadBalancerConfig::default();
        config.strategy = LoadBalancingStrategy::LoadBased;
        let lb = IntelligentLoadBalancer::new(config);
        
        lb.add_node("healthy_node".to_string(), 1.0).await;
        lb.add_node("unhealthy_node".to_string(), 1.0).await;

        // 标记一个节点为不健康
        lb.update_node_health("unhealthy_node", false, 1000.0).await;

        // 请求应该只路由到健康节点
        for _ in 0..5 {
            let result = lb.route_request("test").await.unwrap();
            assert_eq!(result.target_node, "healthy_node");
        }
    }

    #[tokio::test]
    async fn test_load_balance_report() {
        let config = LoadBalancerConfig::default();
        let lb = IntelligentLoadBalancer::new(config);
        
        lb.add_node("node1".to_string(), 1.0).await;
        lb.add_node("node2".to_string(), 1.0).await;

        // 模拟一些请求
        for _ in 0..10 {
            lb.route_request("test").await;
        }

        let report = lb.check_load_balance().await;
        assert!(report.total_requests > 0);
    }
}