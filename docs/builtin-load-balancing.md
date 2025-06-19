# 🍇 内置负载均衡 - 替代 nginx 解决方案

## 概述

Grape Vector Database 现在提供内置的智能负载均衡功能，消除了对外部负载均衡器（如 nginx 或 HAProxy）的依赖。这使得多节点部署更加简单，并提供了与 Qdrant 一致的分布式行为。

## 🚀 快速开始

### 启用内置负载均衡

```rust
use grape_vector_db::distributed::{
    ClusterService, ClusterServiceConfig, LoadBalancingStrategy
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建存储
    let storage = Arc::new(AdvancedStorage::new("./data").await?);
    
    // 配置集群服务，启用内置负载均衡
    let mut config = ClusterServiceConfig::default();
    config.enable_builtin_load_balancer = true;
    config.load_balancer_config.strategy = LoadBalancingStrategy::LoadBased;
    
    // 创建并启动集群服务
    let cluster_service = ClusterService::new(config, storage).await?;
    cluster_service.start().await?;
    
    println!("🎉 集群服务已启动，内置负载均衡已激活！");
    Ok(())
}
```

## 🔧 负载均衡策略

### 1. 轮询 (Round Robin)
```rust
config.load_balancer_config.strategy = LoadBalancingStrategy::RoundRobin;
```
- **特点**: 简单均匀分发请求
- **适用场景**: 节点性能相似的环境

### 2. 加权轮询 (Weighted Round Robin)
```rust
config.load_balancer_config.strategy = LoadBalancingStrategy::WeightedRoundRobin;
```
- **特点**: 根据节点权重分发请求
- **适用场景**: 节点性能差异较大的环境

### 3. 最少连接 (Least Connections)
```rust
config.load_balancer_config.strategy = LoadBalancingStrategy::LeastConnections;
```
- **特点**: 请求路由到连接数最少的节点
- **适用场景**: 连接持续时间不均匀的场景

### 4. 基于负载 (Load Based) ⭐ **推荐**
```rust
config.load_balancer_config.strategy = LoadBalancingStrategy::LoadBased;
```
- **特点**: 综合考虑CPU、内存、响应时间和连接数
- **适用场景**: 大多数生产环境

## 📊 智能路由功能

### 请求类型感知路由
```rust
// 获取请求路由器
if let Some(router) = cluster_service.get_request_router() {
    let route_info = RequestRouteInfo {
        request_id: "search-001".to_string(),
        request_type: RequestType::VectorSearch,
        priority: 8, // 高优先级
        timeout: Duration::from_secs(5),
        requires_consistency: false,
    };
    
    // 执行向量搜索
    let result = router.execute_vector_search(search_request, route_info).await?;
    println!("请求路由到节点: {}", result.executed_node);
}
```

### 自动故障转移
```rust
// 配置故障转移
config.routing_config.enable_failover = true;
config.routing_config.max_retries = 3;
config.routing_config.request_timeout_ms = 5000;
```

## 🏥 健康监控

### 获取集群健康状态
```rust
let health = cluster_service.get_cluster_health().await?;
println!("总节点数: {}", health.total_nodes);
println!("健康节点数: {}", health.healthy_nodes);
println!("健康百分比: {:.1}%", health.health_percentage);

for node in &health.node_details {
    println!("节点 {}: {} (响应时间: {:.2}ms)", 
             node.node_id, 
             if node.is_healthy { "健康" } else { "不健康" },
             node.avg_response_time_ms);
}
```

### 获取负载均衡状态
```rust
if let Some(lb_status) = cluster_service.get_load_balance_status().await {
    println!("负载是否均衡: {}", lb_status.is_balanced);
    println!("最大偏差: {:.3}", lb_status.deviation);
    
    for (node_id, percentage) in &lb_status.node_distributions {
        println!("节点 {}: {:.1}% 的请求", node_id, percentage);
    }
}
```

## 🔍 服务发现

### 自动节点发现
```rust
config.service_discovery_config = ServiceDiscoveryConfig {
    enable_auto_discovery: true,
    seed_nodes: vec![
        "node1.example.com:8080".to_string(),
        "node2.example.com:8080".to_string(),
        "node3.example.com:8080".to_string(),
    ],
    discovery_interval_secs: 60,
    health_check_interval_secs: 30,
};
```

### 手动添加节点
```rust
let new_node = NodeInfo {
    id: "node-4".to_string(),
    address: "192.168.1.104".to_string(),
    port: 8080,
    role: NodeRole::Follower,
    ..Default::default()
};

cluster_service.add_node(new_node).await?;
```

## 🆚 与 nginx 对比

| 特性 | Grape 内置负载均衡 | nginx |
|------|-------------------|-------|
| **配置复杂度** | ✅ 零配置，开箱即用 | ❌ 需要额外配置文件 |
| **网络延迟** | ✅ 零额外跳跃 | ❌ 增加一次网络跳跃 |
| **健康检查** | ✅ 应用层感知检查 | ⚠️ 基础TCP/HTTP检查 |
| **负载均衡算法** | ✅ 智能基于负载的算法 | ⚠️ 静态算法 |
| **故障转移** | ✅ 亚秒级自动转移 | ⚠️ 依赖配置的检查间隔 |
| **维护成本** | ✅ 零额外维护 | ❌ 需要单独维护nginx |
| **监控集成** | ✅ 内置指标和监控 | ❌ 需要额外监控配置 |
| **动态权重调整** | ✅ 基于实时性能自动调整 | ❌ 静态权重 |

## 🚀 部署指南

### 1. 单节点启动（开发环境）
```bash
# 启动主节点
./grape-vector-db --config cluster.toml --node-id primary
```

### 2. 多节点集群（生产环境）
```bash
# 节点1 (Leader)
./grape-vector-db --config cluster.toml --node-id node-1 --port 8080

# 节点2 (Follower)
./grape-vector-db --config cluster.toml --node-id node-2 --port 8081 \
  --seed-nodes node-1:8080

# 节点3 (Follower)  
./grape-vector-db --config cluster.toml --node-id node-3 --port 8082 \
  --seed-nodes node-1:8080,node-2:8081
```

### 3. 客户端连接
```rust
// 客户端可以连接到任何节点
let client = GrapeVectorClient::new("http://node-1:8080").await?;
// 或者
let client = GrapeVectorClient::new("http://node-2:8081").await?;
// 或者  
let client = GrapeVectorClient::new("http://node-3:8082").await?;

// 所有请求都会被智能路由到最佳节点
let results = client.search(query).await?;
```

## ⚙️ 高级配置

### 自定义负载均衡器配置
```rust
config.load_balancer_config = LoadBalancerConfig {
    strategy: LoadBalancingStrategy::LoadBased,
    health_check_interval_secs: 15,     // 健康检查间隔
    unhealthy_threshold_ms: 3000,       // 不健康阈值
    weight_update_interval_secs: 30,    // 权重更新间隔
    max_retries: 3,                     // 最大重试次数
    failover_timeout_ms: 1000,          // 故障转移超时
};
```

### 自定义路由配置
```rust
config.routing_config = RoutingConfig {
    enable_failover: true,
    max_retries: 3,
    request_timeout_ms: 5000,
    connection_pool_size: 20,
    enable_request_cache: true,
    cache_ttl_secs: 300,
};
```

## 📈 性能监控

### 获取路由性能指标
```rust
if let Some(router) = cluster_service.get_request_router() {
    let metrics = router.get_routing_metrics().await;
    
    println!("性能指标:");
    println!("  总请求数: {}", metrics.total_requests);
    println!("  成功率: {:.2}%", 
             metrics.successful_requests as f64 / metrics.total_requests as f64 * 100.0);
    println!("  平均响应时间: {:.2}ms", metrics.avg_response_time_ms);
    println!("  故障转移次数: {}", metrics.failover_count);
    println!("  缓存命中率: {:.2}%", 
             metrics.cache_hits as f64 / (metrics.cache_hits + metrics.cache_misses) as f64 * 100.0);
}
```

## 🎯 Qdrant 兼容性

内置负载均衡器提供了与 Qdrant 一致的分布式行为：

1. **API兼容**: 100% Qdrant API 兼容
2. **客户端透明**: 客户端无需感知负载均衡
3. **一致性保证**: 强一致性和最终一致性选项
4. **动态扩缩容**: 支持节点的动态添加和移除

## 🛠️ 故障排除

### 常见问题

**Q: 节点无法加入集群？**
```bash
# 检查网络连通性
curl http://seed-node:8080/health

# 检查配置
grep -E "(seed_nodes|port)" cluster.toml
```

**Q: 负载不均衡？**
```rust
// 检查负载均衡状态
let lb_status = cluster_service.get_load_balance_status().await?;
if !lb_status.is_balanced {
    println!("负载不均衡，偏差: {:.3}", lb_status.deviation);
    // 考虑调整负载均衡策略或检查节点健康状态
}
```

**Q: 响应时间过长？**
```rust
// 检查节点健康状态
let health = cluster_service.get_cluster_health().await?;
for node in &health.node_details {
    if node.avg_response_time_ms > 1000.0 {
        println!("节点 {} 响应时间过长: {:.2}ms", node.node_id, node.avg_response_time_ms);
    }
}
```

## 🎉 总结

通过使用 Grape Vector Database 的内置负载均衡功能，您可以：

✅ **简化部署**: 无需配置和维护外部负载均衡器  
✅ **提高性能**: 消除额外网络跳跃，智能路由算法  
✅ **增强可靠性**: 亚秒级故障检测和转移  
✅ **降低成本**: 减少基础设施复杂性和维护成本  
✅ **提升监控**: 内置丰富的性能指标和健康监控  

立即开始使用内置负载均衡，让您的向量数据库集群更加智能和高效！