/// 分布式系统集成测试
/// 测试Raft共识算法和分片管理器的基本功能

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tempfile::TempDir;

use grape_vector_db::advanced_storage::{AdvancedStorage, AdvancedStorageConfig};
use grape_vector_db::distributed::raft::{RaftConfig, RaftNode};
use grape_vector_db::distributed::shard::{ShardManager, ShardConfig, ConsistentHashRing};

#[tokio::test]
async fn test_raft_node_creation() {
    // 测试Raft节点创建
    let config = RaftConfig {
        node_id: "node1".to_string(),
        peers: vec!["node2".to_string(), "node3".to_string()],
        election_timeout_ms: 150,
        heartbeat_interval_ms: 50,
        log_compaction_threshold: 1000,
        snapshot_interval: 100,
    };

    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let mut storage_config = AdvancedStorageConfig::default();
    storage_config.db_path = temp_dir.path().to_path_buf();
    let storage = Arc::new(AdvancedStorage::new(storage_config).expect("创建存储失败"));
    
    let raft_node = RaftNode::new(config, storage);
    
    // 验证节点状态
    assert_eq!(raft_node.get_node_id(), "node1");
    assert_eq!(raft_node.get_term().await, 0);
    
    println!("✅ Raft节点创建测试通过");
}

#[tokio::test]
async fn test_consistent_hash_ring() {
    // 测试一致性哈希环
    let mut hash_ring = ConsistentHashRing::new(100);
    
    // 添加节点
    hash_ring.add_node("node1".to_string(), 1);
    hash_ring.add_node("node2".to_string(), 1);
    hash_ring.add_node("node3".to_string(), 1);
    
    // 测试路由
    let key1 = "test_key_1";
    let key2 = "test_key_2";
    let key3 = "test_key_3";
    
    let node1 = hash_ring.get_node(key1);
    let node2 = hash_ring.get_node(key2);
    let node3 = hash_ring.get_node(key3);
    
    assert!(node1.is_some());
    assert!(node2.is_some());
    assert!(node3.is_some());
    
    // 测试相同键的一致性
    let node1_again = hash_ring.get_node(key1);
    assert_eq!(node1, node1_again);
    
    // 获取统计信息
    let stats = hash_ring.get_stats();
    assert_eq!(stats.get("physical_nodes_count").unwrap().as_u64().unwrap(), 3);
    assert_eq!(stats.get("virtual_nodes_count").unwrap().as_u64().unwrap(), 300);
    
    println!("✅ 一致性哈希环测试通过");
}

#[tokio::test]
async fn test_shard_manager_initialization() {
    // 测试分片管理器初始化
    let config = ShardConfig::default();
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let mut storage_config = AdvancedStorageConfig::default();
    storage_config.db_path = temp_dir.path().to_path_buf();
    let storage = Arc::new(AdvancedStorage::new(storage_config).expect("创建存储失败"));
    
    let shard_manager = ShardManager::new(config, storage, "node1".to_string());
    
    // 初始化分片
    let cluster_nodes = vec!["node1".to_string(), "node2".to_string(), "node3".to_string()];
    shard_manager.initialize_shards(cluster_nodes).await.expect("分片初始化失败");
    
    // 测试分片路由
    let key = "test_vector_key";
    let shard_id = shard_manager.get_shard_id(key);
    assert!(shard_id < 256); // 默认分片数量
    
    // 测试相同键的一致性
    let shard_id_again = shard_manager.get_shard_id(key);
    assert_eq!(shard_id, shard_id_again);
    
    println!("✅ 分片管理器初始化测试通过");
}

#[tokio::test]
async fn test_shard_health_monitoring() {
    // 测试分片健康监控
    let config = ShardConfig::default();
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let mut storage_config = AdvancedStorageConfig::default();
    storage_config.db_path = temp_dir.path().to_path_buf();
    let storage = Arc::new(AdvancedStorage::new(storage_config).expect("创建存储失败"));
    
    let shard_manager = ShardManager::new(config, storage, "node1".to_string());
    
    // 初始化分片
    let cluster_nodes = vec!["node1".to_string()];
    shard_manager.initialize_shards(cluster_nodes).await.expect("分片初始化失败");
    
    // 收集健康状态
    let health_statuses = shard_manager.collect_all_shard_health().await.expect("收集健康状态失败");
    
    // 验证健康状态
    for status in &health_statuses {
        assert!(status.is_healthy); // 新创建的分片应该是健康的
        assert!(status.issues.is_empty());
        assert!(!status.metrics.is_empty());
    }
    
    // 获取集群负载统计
    let cluster_stats = shard_manager.get_cluster_load_stats().await.expect("获取集群统计失败");
    assert!(cluster_stats.local_shards > 0);
    
    println!("✅ 分片健康监控测试通过, 监控了 {} 个分片", health_statuses.len());
}

#[tokio::test]
async fn test_raft_state_persistence() {
    // 测试Raft状态持久化
    let config = RaftConfig {
        node_id: "test_node".to_string(),
        peers: vec!["peer1".to_string()],
        election_timeout_ms: 150,
        heartbeat_interval_ms: 50,
        log_compaction_threshold: 1000,
        snapshot_interval: 100,
    };

    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let mut storage_config = AdvancedStorageConfig::default();
    storage_config.db_path = temp_dir.path().to_path_buf();
    let storage = Arc::new(AdvancedStorage::new(storage_config).expect("创建存储失败"));
    
    let raft_node = RaftNode::new(config.clone(), storage.clone());
    
    // 测试状态恢复（应该不会出错，即使没有持久化的状态）
    // 这个测试主要是验证代码路径没有错误
    let result = raft_node.restore_state().await;
    assert!(result.is_ok());
    
    // 测试状态持久化
    let result = raft_node.persist_state().await;
    assert!(result.is_ok());
    
    println!("✅ Raft状态持久化测试通过");
}

#[tokio::test]
async fn test_shard_rebalancing() {
    // 测试分片重平衡
    let config = ShardConfig {
        shard_count: 8, // 使用较小的分片数量便于测试
        ..Default::default()
    };
    
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let mut storage_config = AdvancedStorageConfig::default();
    storage_config.db_path = temp_dir.path().to_path_buf();
    let storage = Arc::new(AdvancedStorage::new(storage_config).expect("创建存储失败"));
    
    let shard_manager = ShardManager::new(config, storage, "node1".to_string());
    
    // 初始化分片
    let cluster_nodes = vec!["node1".to_string(), "node2".to_string()];
    shard_manager.initialize_shards(cluster_nodes.clone()).await.expect("分片初始化失败");
    
    // 测试重平衡
    let migrations = shard_manager.rebalance_shards(cluster_nodes).await.expect("重平衡失败");
    
    // 重平衡可能返回空的迁移列表（如果负载已经平衡）
    println!("✅ 分片重平衡测试通过, 生成了 {} 个迁移计划", migrations.len());
    
    // 验证迁移计划的合理性
    for migration in &migrations {
        assert!(!migration.from_node.is_empty());
        assert!(!migration.to_node.is_empty());
        assert_ne!(migration.from_node, migration.to_node);
        assert!(migration.estimated_size > 0);
    }
}

#[tokio::test]
async fn test_log_compaction() {
    // 测试日志压缩
    let config = RaftConfig {
        node_id: "compact_test_node".to_string(),
        peers: vec![],
        election_timeout_ms: 150,
        heartbeat_interval_ms: 50,
        log_compaction_threshold: 1000,
        snapshot_interval: 100,
    };

    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let mut storage_config = AdvancedStorageConfig::default();
    storage_config.db_path = temp_dir.path().to_path_buf();
    let storage = Arc::new(AdvancedStorage::new(storage_config).expect("创建存储失败"));
    
    let raft_node = RaftNode::new(config, storage);
    
    // 测试日志压缩检查
    let should_compact = raft_node.should_compact_log().await;
    assert!(!should_compact); // 新节点不应该需要压缩
    
    // 测试自动压缩（应该什么都不做）
    let result = raft_node.auto_compact_log().await;
    assert!(result.is_ok());
    
    println!("✅ 日志压缩测试通过");
}

/// 性能基准测试
#[tokio::test]
async fn benchmark_hash_ring_performance() {
    let mut hash_ring = ConsistentHashRing::new(100);
    
    // 添加多个节点
    for i in 0..10 {
        hash_ring.add_node(format!("node{}", i), 1);
    }
    
    let start = std::time::Instant::now();
    
    // 测试大量查询
    for i in 0..1000 {
        let key = format!("test_key_{}", i);
        let _node = hash_ring.get_node(&key);
    }
    
    let duration = start.elapsed();
    println!("✅ 哈希环性能测试: 1000次查询耗时 {:?}", duration);
    
    // 应该在合理时间内完成
    assert!(duration < Duration::from_millis(100));
}