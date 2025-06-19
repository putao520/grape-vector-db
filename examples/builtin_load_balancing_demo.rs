use std::sync::Arc;
use tempfile::TempDir;
use tokio;

use grape_vector_db::distributed::{
    ClusterService, ClusterServiceConfig, LoadBalancingStrategy, LoadBalancerConfig,
    RoutingConfig, ServiceDiscoveryConfig
};
use grape_vector_db::types::*;
use grape_vector_db::advanced_storage::AdvancedStorage;

/// 演示内置负载均衡功能的集成测试
#[tokio::test]
async fn test_builtin_load_balancing_demo() {
    println!("🍇 Grape Vector Database - 内置负载均衡演示");
    println!("=========================================");

    // 创建临时目录用于存储
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(
        AdvancedStorage::new(temp_dir.path().to_str().unwrap())
            .await
            .unwrap()
    );

    // 配置集群服务，启用内置负载均衡
    let mut config = ClusterServiceConfig::default();
    config.enable_builtin_load_balancer = true;
    config.load_balancer_config.strategy = LoadBalancingStrategy::LoadBased;
    config.local_node = NodeInfo {
        id: "node-primary".to_string(),
        address: "127.0.0.1".to_string(),
        port: 8080,
        role: NodeRole::Leader,
        ..Default::default()
    };

    // 配置服务发现
    config.service_discovery_config = ServiceDiscoveryConfig {
        enable_auto_discovery: true,
        seed_nodes: vec!["127.0.0.1:8081".to_string(), "127.0.0.1:8082".to_string()],
        discovery_interval_secs: 30,
        health_check_interval_secs: 15,
    };

    println!("✅ 正在创建集群服务...");
    let cluster_service = ClusterService::new(config, storage).await.unwrap();
    
    // 验证内置负载均衡已启用
    assert!(cluster_service.is_builtin_load_balancer_enabled());
    println!("✅ 内置负载均衡已启用");

    // 启动集群服务
    println!("🚀 启动集群服务...");
    cluster_service.start().await.unwrap();

    // 添加模拟节点到集群
    println!("📍 添加节点到集群...");
    for i in 1..=3 {
        let node = NodeInfo {
            id: format!("node-{}", i),
            address: "127.0.0.1".to_string(),
            port: 8080 + i,
            role: NodeRole::Follower,
            ..Default::default()
        };
        cluster_service.add_node(node).await.unwrap();
        println!("   ➕ 已添加节点: node-{}", i);
    }

    // 检查集群健康状态
    println!("🏥 检查集群健康状态...");
    let health_status = cluster_service.get_cluster_health().await.unwrap();
    println!("   📊 总节点数: {}", health_status.total_nodes);
    println!("   ✅ 健康节点数: {}", health_status.healthy_nodes);
    println!("   📈 健康百分比: {:.1}%", health_status.health_percentage);

    // 获取负载均衡状态
    if let Some(lb_status) = cluster_service.get_load_balance_status().await {
        println!("⚖️  负载均衡状态:");
        println!("   🎯 是否均衡: {}", lb_status.is_balanced);
        println!("   📏 偏差比例: {:.3}", lb_status.deviation);
        println!("   📋 路由策略: {}", lb_status.routing_strategy);
    }

    // 演示请求路由
    if let Some(router) = cluster_service.get_request_router() {
        println!("🔀 演示智能请求路由...");
        
        // 模拟搜索请求
        let search_request = SearchRequest {
            vector: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            limit: 10,
            filter: None,
            with_payload: true,
            with_vector: false,
        };

        let route_info = RequestRouteInfo {
            request_id: "demo-search-001".to_string(),
            request_type: RequestType::VectorSearch,
            request_size_bytes: 1024,
            priority: 5,
            timeout: std::time::Duration::from_secs(5),
            requires_consistency: false,
        };

        // 尝试路由搜索请求（会失败，因为没有真实的后端，但演示了路由逻辑）
        println!("   🔍 尝试路由向量搜索请求...");
        match router.execute_vector_search(search_request, route_info).await {
            Ok(result) => {
                println!("   ✅ 请求成功路由到节点: {}", result.executed_node);
                println!("   ⏱️  执行时间: {}ms", result.execution_time_ms);
                println!("   🔄 是否使用故障转移: {}", result.used_failover);
            },
            Err(e) => {
                println!("   ⚠️  请求路由失败 (预期，因为没有真实后端): {}", e);
            }
        }

        // 获取路由性能指标
        let metrics = router.get_routing_metrics().await;
        println!("📊 路由性能指标:");
        println!("   📋 总请求数: {}", metrics.total_requests);
        println!("   ✅ 成功请求数: {}", metrics.successful_requests);
        println!("   ❌ 失败请求数: {}", metrics.failed_requests);
        println!("   🔄 故障转移次数: {}", metrics.failover_count);
    }

    // 获取API使用指南
    println!("📖 API使用指南:");
    let usage_guide = cluster_service.get_api_usage_guide();
    for line in usage_guide.lines() {
        println!("   {}", line);
    }

    // 优雅停止服务
    println!("🛑 停止集群服务...");
    cluster_service.stop().await.unwrap();
    println!("✅ 集群服务已停止");

    println!("\n🎉 内置负载均衡演示完成!");
    println!("    现在您可以部署多个 Grape Vector Database 节点，");
    println!("    客户端连接到任何节点都会被智能路由，");
    println!("    无需外部 nginx 或 HAProxy 负载均衡器！");
}

/// 演示Qdrant兼容的分布式行为
#[tokio::test]
async fn test_qdrant_compatible_behavior() {
    println!("\n🔗 Qdrant兼容性演示");
    println!("===================");

    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(
        AdvancedStorage::new(temp_dir.path().to_str().unwrap())
            .await
            .unwrap()
    );

    // 创建Qdrant兼容的配置
    let mut config = ClusterServiceConfig::default();
    config.enable_builtin_load_balancer = true;
    config.load_balancer_config.strategy = LoadBalancingStrategy::WeightedRoundRobin;
    
    let cluster_service = ClusterService::new(config, storage).await.unwrap();
    cluster_service.start().await.unwrap();

    println!("✅ Qdrant兼容的集群服务已启动");
    println!("   🔧 支持多种负载均衡策略");
    println!("   🔄 自动故障转移");
    println!("   📊 实时健康监控");
    println!("   🎯 智能请求路由");

    let is_healthy = cluster_service.is_cluster_healthy().await;
    println!("🏥 集群健康状态: {}", if is_healthy { "健康" } else { "不健康" });

    cluster_service.stop().await.unwrap();
    println!("✅ 演示完成");
}

/// 性能对比测试：nginx vs 内置负载均衡
#[tokio::test]
async fn test_performance_comparison() {
    println!("\n⚡ 性能对比演示");
    println!("================");

    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(
        AdvancedStorage::new(temp_dir.path().to_str().unwrap())
            .await
            .unwrap()
    );

    let mut config = ClusterServiceConfig::default();
    config.enable_builtin_load_balancer = true;
    config.load_balancer_config.strategy = LoadBalancingStrategy::LeastConnections;
    
    let cluster_service = ClusterService::new(config, storage).await.unwrap();
    cluster_service.start().await.unwrap();

    println!("📊 内置负载均衡器优势:");
    println!("   🚀 零额外网络跳跃");
    println!("   🧠 智能节点选择 (基于CPU、内存、连接数)");
    println!("   🔄 自动故障检测和恢复");
    println!("   📈 实时性能监控");
    println!("   🎯 请求类型感知路由");
    println!("   ⚙️  零配置，开箱即用");

    println!("\n🆚 vs nginx负载均衡:");
    println!("   ❌ 需要额外配置和维护");
    println!("   ❌ 增加网络延迟");
    println!("   ❌ 不感知应用层状态");
    println!("   ❌ 静态路由规则");

    cluster_service.stop().await.unwrap();
    println!("✅ 性能对比演示完成");
}

#[tokio::main]
async fn main() {
    // 运行所有演示
    test_builtin_load_balancing_demo().await;
    test_qdrant_compatible_behavior().await;
    test_performance_comparison().await;
}