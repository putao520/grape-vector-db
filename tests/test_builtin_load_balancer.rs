use grape_vector_db::distributed::{
    IntelligentLoadBalancer, LoadBalancerConfig, LoadBalancingStrategy,
    ClusterAwareRequestRouter, RoutingConfig, 
    DistributedNetworkClient
};
use std::sync::Arc;

#[tokio::test]
async fn test_builtin_load_balancer_basic() {
    println!("🍇 测试内置负载均衡器基本功能");
    
    // 创建负载均衡器
    let mut config = LoadBalancerConfig::default();
    config.strategy = LoadBalancingStrategy::LoadBased;
    
    let load_balancer = Arc::new(IntelligentLoadBalancer::new(config));
    
    // 添加节点
    load_balancer.add_node("node-1".to_string(), 1.0).await;
    load_balancer.add_node("node-2".to_string(), 0.8).await;
    load_balancer.add_node("node-3".to_string(), 0.6).await;
    
    // 验证节点状态
    let nodes = load_balancer.get_all_nodes_status().await;
    assert_eq!(nodes.len(), 3);
    println!("✅ 成功添加 {} 个节点", nodes.len());
    
    // 测试路由
    for i in 0..5 {
        if let Some(route_result) = load_balancer.route_request(&format!("request-{}", i)).await {
            println!("   🔀 请求 {} 路由到节点: {}", i, route_result.target_node);
            assert!(!route_result.target_node.is_empty());
        }
    }
    
    // 测试负载均衡报告
    let report = load_balancer.check_load_balance().await;
    println!("📊 负载均衡报告:");
    println!("   是否均衡: {}", report.is_balanced);
    println!("   总请求数: {}", report.total_requests);
    
    println!("✅ 内置负载均衡器测试通过!");
}

#[tokio::test]
async fn test_request_routing() {
    println!("🔀 测试请求路由功能");
    
    let lb_config = LoadBalancerConfig::default();
    let load_balancer = Arc::new(IntelligentLoadBalancer::new(lb_config));
    let network_client = Arc::new(DistributedNetworkClient::new());
    let routing_config = RoutingConfig::default();
    
    let router = ClusterAwareRequestRouter::new(
        load_balancer.clone(),
        network_client,
        routing_config,
    );
    
    // 添加节点到负载均衡器
    load_balancer.add_node("node-1".to_string(), 1.0).await;
    load_balancer.add_node("node-2".to_string(), 1.0).await;
    
    // 测试集群健康状态
    let health = router.get_cluster_health().await;
    println!("🏥 集群健康状态:");
    println!("   总节点数: {}", health.total_nodes);
    println!("   健康节点数: {}", health.healthy_nodes);
    println!("   健康百分比: {:.1}%", health.health_percentage);
    
    // 测试负载均衡状态
    let lb_status = router.get_load_balance_status().await;
    println!("⚖️  负载均衡状态:");
    println!("   是否均衡: {}", lb_status.is_balanced);
    println!("   路由策略: {}", lb_status.routing_strategy);
    
    // 测试路由指标
    let metrics = router.get_routing_metrics().await;
    println!("📈 路由指标:");
    println!("   总请求数: {}", metrics.total_requests);
    println!("   成功请求数: {}", metrics.successful_requests);
    
    println!("✅ 请求路由测试通过!");
}

#[tokio::test]
async fn test_load_balancing_strategies() {
    println!("🎯 测试负载均衡策略");
    
    let strategies = vec![
        LoadBalancingStrategy::RoundRobin,
        LoadBalancingStrategy::WeightedRoundRobin,
        LoadBalancingStrategy::LeastConnections,
        LoadBalancingStrategy::LoadBased,
    ];
    
    for strategy in strategies {
        let mut config = LoadBalancerConfig::default();
        config.strategy = strategy.clone();
        
        let load_balancer = IntelligentLoadBalancer::new(config);
        
        // 添加测试节点
        load_balancer.add_node("node-1".to_string(), 1.0).await;
        load_balancer.add_node("node-2".to_string(), 0.8).await;
        
        // 测试路由
        let mut route_counts = std::collections::HashMap::new();
        for _ in 0..10 {
            if let Some(result) = load_balancer.route_request("test").await {
                *route_counts.entry(result.target_node).or_insert(0) += 1;
            }
        }
        
        println!("   📊 策略 {:?} 路由分布: {:?}", strategy, route_counts);
        assert!(!route_counts.is_empty());
    }
    
    println!("✅ 负载均衡策略测试通过!");
}

#[tokio::main]
async fn main() {
    println!("🍇 Grape Vector Database - 内置负载均衡测试");
    println!("==========================================");
    
    test_builtin_load_balancer_basic().await;
    println!();
    test_request_routing().await;
    println!();
    test_load_balancing_strategies().await;
    
    println!("\n🎉 所有测试通过! 内置负载均衡功能正常工作");
    println!("   现在您可以在多节点部署中使用内置负载均衡，");
    println!("   无需依赖外部 nginx 或 HAProxy！");
}