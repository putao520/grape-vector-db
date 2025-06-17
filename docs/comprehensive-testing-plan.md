# 🧪 Grape Vector Database - 综合测试方案

## 📋 测试概述

本文档详细规划了Grape Vector Database在四种不同模式下的完整测试方案：
- **内嵌模式** (Embedded Mode)
- **单机模式** (Standalone Mode) 
- **3集群模式** (3-Node Cluster Mode)
- **6集群模式** (6-Node Cluster Mode)

重点测试Raft共识算法和Resharding分片算法的正确性、性能和容错能力。

## 🎯 测试目标

### 核心算法测试
- ✅ **Raft共识算法**: 领导者选举、日志复制、故障恢复
- ✅ **Resharding算法**: 一致性哈希、分片迁移、负载均衡

### 模式覆盖测试
- ✅ **内嵌模式**: 进程内集成、同步API、生命周期管理
- ✅ **单机模式**: 独立服务、gRPC接口、本地存储
- ✅ **3集群模式**: 基本分布式、故障容错、数据一致性
- ✅ **6集群模式**: 高可用、分片负载、扩展性能

## 📊 测试矩阵

| 测试类型 | 内嵌模式 | 单机模式 | 3集群模式 | 6集群模式 | Raft测试 | 分片测试 |
|----------|----------|----------|-----------|-----------|----------|----------|
| 功能测试 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 性能测试 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 容错测试 | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 一致性测试 | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ |
| 扩展性测试 | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ |

## 🏗️ 测试架构

### 测试基础设施
```rust
/// 测试集群管理器
pub struct TestCluster {
    nodes: Vec<TestNode>,
    cluster_type: ClusterType,
    network_simulator: NetworkSimulator,
    chaos_engine: ChaosEngine,
}

/// 测试节点
pub struct TestNode {
    node_id: String,
    raft_node: RaftNode,
    shard_manager: ShardManager,
    storage: AdvancedStorage,
    grpc_server: Option<GrpcServer>,
}

/// 集群类型
#[derive(Debug, Clone)]
pub enum ClusterType {
    Embedded,
    Standalone,
    ThreeNode,
    SixNode,
}
```

### 网络模拟器
```rust
/// 网络模拟器 - 用于故障注入
pub struct NetworkSimulator {
    partitions: HashMap<NodeId, Vec<NodeId>>,
    latency_map: HashMap<(NodeId, NodeId), Duration>,
    failure_nodes: HashSet<NodeId>,
}
```

## 📋 测试计划详述

### 1. 内嵌模式测试 (Embedded Mode)

#### 1.1 基本功能测试
```rust
#[test_suite("embedded_basic")]
mod embedded_basic_tests {
    #[test]
    fn test_embedded_startup_shutdown() {
        // 测试内嵌模式启动和关闭
        // 验证: 启动时间 < 1s, 关闭时间 < 5s
    }
    
    #[test]
    fn test_embedded_sync_api() {
        // 测试同步API接口
        // 验证: 所有CRUD操作的同步版本
    }
    
    #[test]
    fn test_embedded_lifecycle_management() {
        // 测试生命周期管理
        // 验证: 资源正确初始化和清理
    }
}
```

#### 1.2 性能测试
```rust
#[test_suite("embedded_performance")]
mod embedded_performance_tests {
    #[test]
    fn test_embedded_memory_usage() {
        // 测试内存使用
        // 目标: 100K向量 < 500MB内存
    }
    
    #[test]
    fn test_embedded_concurrent_access() {
        // 测试并发访问
        // 目标: 1000并发连接正常工作
    }
}
```

### 2. 单机模式测试 (Standalone Mode)

#### 2.1 服务测试
```rust
#[test_suite("standalone_service")]
mod standalone_service_tests {
    #[test]
    async fn test_grpc_server_startup() {
        // 测试gRPC服务启动
        // 验证: 服务正常监听和响应
    }
    
    #[test]
    async fn test_rest_api_compatibility() {
        // 测试REST API兼容性
        // 验证: Qdrant兼容接口
    }
}
```

#### 2.2 持久化测试
```rust
#[test_suite("standalone_persistence")]
mod standalone_persistence_tests {
    #[test]
    async fn test_data_persistence() {
        // 测试数据持久化
        // 验证: 重启后数据完整性
    }
    
    #[test]
    async fn test_index_recovery() {
        // 测试索引恢复
        // 验证: 索引重建的正确性
    }
}
```

### 3. Raft共识算法测试

#### 3.1 核心功能测试
```rust
#[test_suite("raft_consensus")]
mod raft_consensus_tests {
    #[test]
    async fn test_leader_election() {
        // 测试领导者选举
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        
        // 启动集群
        cluster.start_all_nodes().await;
        
        // 等待选举完成
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 验证只有一个领导者
        let leaders = cluster.get_leaders().await;
        assert_eq!(leaders.len(), 1);
        
        // 验证其他节点为跟随者
        let followers = cluster.get_followers().await;
        assert_eq!(followers.len(), 2);
    }
    
    #[test]
    async fn test_log_replication() {
        // 测试日志复制
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        cluster.start_all_nodes().await;
        
        let leader = cluster.wait_for_leader().await;
        
        // 向领导者写入数据
        let entries = vec![
            b"test_entry_1".to_vec(),
            b"test_entry_2".to_vec(),
            b"test_entry_3".to_vec(),
        ];
        
        for entry in &entries {
            leader.propose_entry(entry.clone()).await.unwrap();
        }
        
        // 等待复制完成
        cluster.wait_for_log_sync().await;
        
        // 验证所有节点日志一致
        for node in cluster.nodes() {
            assert_eq!(node.get_logs().await, entries);
        }
    }
    
    #[test]
    async fn test_network_partition() {
        // 测试网络分区场景
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 创建分区: [0,1,2] vs [3,4,5]
        cluster.create_partition(vec![0, 1, 2], vec![3, 4, 5]).await;
        
        // 验证分区后无法达成共识
        assert!(!cluster.can_reach_consensus().await);
        
        // 愈合分区
        cluster.heal_partition().await;
        
        // 验证恢复后可以达成共识
        assert!(cluster.can_reach_consensus().await);
    }
    
    #[test]
    async fn test_leader_failure_recovery() {
        // 测试领导者故障恢复
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        cluster.start_all_nodes().await;
        
        let leader = cluster.wait_for_leader().await;
        let leader_id = leader.get_node_id();
        
        // 停止领导者
        cluster.stop_node(&leader_id).await;
        
        // 等待新领导者选出
        let new_leader = cluster.wait_for_leader().await;
        assert_ne!(new_leader.get_node_id(), leader_id);
        
        // 验证集群继续工作
        assert!(cluster.is_available().await);
        
        // 重启原领导者
        cluster.restart_node(&leader_id).await;
        
        // 验证节点重新加入集群
        assert_eq!(cluster.active_node_count().await, 3);
    }
}
```

#### 3.2 边界条件测试
```rust
#[test_suite("raft_edge_cases")]
mod raft_edge_cases_tests {
    #[test]
    async fn test_split_brain_prevention() {
        // 测试脑裂预防
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 创建对称分区
        cluster.create_symmetric_partition().await;
        
        // 验证无法选出领导者
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(cluster.get_leaders().await.len(), 0);
    }
    
    #[test]
    async fn test_concurrent_elections() {
        // 测试并发选举
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        
        // 同时启动所有节点（模拟并发选举）
        cluster.start_all_nodes_concurrently().await;
        
        // 等待选举稳定
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // 验证最终只有一个领导者
        assert_eq!(cluster.get_leaders().await.len(), 1);
    }
}
```

### 4. Resharding分片算法测试

#### 4.1 一致性哈希测试
```rust
#[test_suite("resharding_algorithm")]
mod resharding_algorithm_tests {
    #[test]
    fn test_consistent_hash_distribution() {
        // 测试一致性哈希分布
        let mut hash_ring = ConsistentHashRing::new(1000);
        
        // 添加节点
        for i in 0..6 {
            hash_ring.add_node(format!("node_{}", i), 1);
        }
        
        // 测试1万个键的分布
        let mut distribution = HashMap::new();
        for i in 0..10000 {
            let key = format!("key_{}", i);
            let node = hash_ring.get_node(&key);
            *distribution.entry(node).or_insert(0) += 1;
        }
        
        // 验证分布相对均匀（每个节点在15%-20%之间）
        for (_, count) in distribution {
            let percentage = count as f64 / 10000.0;
            assert!(percentage >= 0.15 && percentage <= 0.20);
        }
    }
    
    #[test]
    async fn test_shard_migration() {
        // 测试分片迁移
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        cluster.start_all_nodes().await;
        
        // 插入测试数据
        let test_vectors = generate_test_vectors(1000);
        for vector in &test_vectors {
            cluster.insert_vector(vector.clone()).await.unwrap();
        }
        
        // 添加新节点触发重分片
        cluster.add_node("node_3").await;
        
        // 等待迁移完成
        cluster.wait_for_resharding_complete().await;
        
        // 验证数据完整性
        for vector in &test_vectors {
            let retrieved = cluster.get_vector(&vector.id).await.unwrap();
            assert_eq!(retrieved, *vector);
        }
        
        // 验证分片均衡
        cluster.verify_shard_balance().await;
    }
    
    #[test]
    async fn test_shard_rebalancing() {
        // 测试分片重平衡
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 模拟数据倾斜
        cluster.create_data_skew("node_0", 80).await; // 80%数据在node_0
        
        // 触发重平衡
        cluster.trigger_rebalancing().await;
        
        // 等待重平衡完成
        cluster.wait_for_rebalancing_complete().await;
        
        // 验证数据均匀分布
        let distribution = cluster.get_data_distribution().await;
        for (_, percentage) in distribution {
            assert!(percentage >= 0.15 && percentage <= 0.20);
        }
    }
}
```

#### 4.2 分片容错测试
```rust
#[test_suite("resharding_fault_tolerance")]
mod resharding_fault_tolerance_tests {
    #[test]
    async fn test_migration_during_node_failure() {
        // 测试迁移过程中节点故障
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 开始分片迁移
        let migration_future = cluster.start_migration("node_0", "node_1");
        
        // 迁移过程中停止目标节点
        tokio::time::sleep(Duration::from_millis(100)).await;
        cluster.stop_node("node_1").await;
        
        // 验证迁移回滚或重定向
        let result = migration_future.await;
        assert!(result.is_err() || cluster.verify_data_integrity().await);
    }
    
    #[test]
    async fn test_concurrent_migrations() {
        // 测试并发迁移
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 同时启动多个迁移
        let migrations = vec![
            cluster.start_migration("node_0", "node_3"),
            cluster.start_migration("node_1", "node_4"),
            cluster.start_migration("node_2", "node_5"),
        ];
        
        // 等待所有迁移完成
        for migration in migrations {
            migration.await.unwrap();
        }
        
        // 验证数据完整性和一致性
        assert!(cluster.verify_data_integrity().await);
        assert!(cluster.verify_data_consistency().await);
    }
}
```

### 5. 集群模式专项测试

#### 5.1 三节点集群测试
```rust
#[test_suite("three_node_cluster")]
mod three_node_cluster_tests {
    #[test]
    async fn test_basic_consensus() {
        // 测试基本共识功能
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        cluster.start_all_nodes().await;
        
        // 基本的读写操作
        let test_doc = create_test_document();
        let doc_id = cluster.insert_document(test_doc.clone()).await.unwrap();
        
        let retrieved = cluster.get_document(&doc_id).await.unwrap();
        assert_eq!(retrieved.content, test_doc.content);
    }
    
    #[test]
    async fn test_one_node_failure() {
        // 测试单节点故障
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        cluster.start_all_nodes().await;
        
        // 停止一个节点
        cluster.stop_node("node_2").await;
        
        // 验证集群仍可提供服务（2/3节点运行）
        assert!(cluster.is_available().await);
        
        // 验证读写功能正常
        let test_doc = create_test_document();
        cluster.insert_document(test_doc).await.unwrap();
    }
    
    #[test]
    async fn test_network_partition_handling() {
        // 测试网络分区处理
        let cluster = TestCluster::new(ClusterType::ThreeNode).await;
        cluster.start_all_nodes().await;
        
        // 创建1+2分区
        cluster.create_partition(vec![0], vec![1, 2]).await;
        
        // 验证多数派（2节点）继续工作
        let majority_partition = cluster.get_partition_nodes(vec![1, 2]).await;
        assert!(majority_partition.is_available().await);
        
        // 验证少数派（1节点）停止服务
        let minority_partition = cluster.get_partition_nodes(vec![0]).await;
        assert!(!minority_partition.is_available().await);
    }
}
```

#### 5.2 六节点集群测试
```rust
#[test_suite("six_node_cluster")]
mod six_node_cluster_tests {
    #[test]
    async fn test_high_availability() {
        // 测试高可用性
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 随机停止2个节点
        cluster.stop_random_nodes(2).await;
        
        // 验证集群仍然可用（4/6节点运行）
        assert!(cluster.is_available().await);
        assert_eq!(cluster.active_node_count().await, 4);
    }
    
    #[test]
    async fn test_load_balancing() {
        // 测试负载均衡
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 并发插入大量数据
        let docs = generate_test_documents(10000);
        let mut handles = Vec::new();
        
        for doc in docs {
            let cluster_clone = cluster.clone();
            let handle = tokio::spawn(async move {
                cluster_clone.insert_document(doc).await
            });
            handles.push(handle);
        }
        
        // 等待所有插入完成
        for handle in handles {
            handle.await.unwrap().unwrap();
        }
        
        // 验证负载分布均匀
        let load_distribution = cluster.get_load_distribution().await;
        assert!(is_balanced(&load_distribution, 0.1)); // 10%偏差内
    }
    
    #[test]
    async fn test_massive_network_partition() {
        // 测试大规模网络分区
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        cluster.start_all_nodes().await;
        
        // 创建复杂分区场景
        cluster.create_complex_partition().await;
        
        // 验证只有多数派能继续工作
        let active_partitions = cluster.get_active_partitions().await;
        assert_eq!(active_partitions.len(), 1);
        assert!(active_partitions[0].node_count() >= 4);
    }
}
```

### 6. 故障注入和混沌测试

#### 6.1 混沌工程测试
```rust
#[test_suite("chaos_engineering")]
mod chaos_engineering_tests {
    #[test]
    async fn test_random_node_failures() {
        // 随机节点故障测试
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        let chaos_engine = ChaosEngine::new(cluster.clone());
        
        // 配置混沌实验
        let experiment = ChaosExperiment::builder()
            .with_duration(Duration::from_minutes(5))
            .with_failure_rate(0.1) // 10%节点故障率
            .with_recovery_time(Duration::from_secs(30))
            .build();
        
        // 执行混沌实验
        chaos_engine.run_experiment(experiment).await;
        
        // 验证系统最终一致性
        assert!(cluster.is_eventually_consistent().await);
    }
    
    #[test]
    async fn test_network_chaos() {
        // 网络混沌测试
        let cluster = TestCluster::new(ClusterType::SixNode).await;
        let chaos_engine = ChaosEngine::new(cluster.clone());
        
        // 注入网络故障
        chaos_engine.inject_network_chaos(NetworkChaos {
            packet_loss: 0.05,    // 5%丢包率
            latency_spike: 100,   // 100ms延迟尖峰
            partition_probability: 0.1, // 10%分区概率
        }).await;
        
        // 在混沌环境中运行工作负载
        let workload = Workload::new()
            .with_read_rate(100)
            .with_write_rate(50)
            .with_duration(Duration::from_minutes(3));
        
        chaos_engine.run_workload(workload).await;
        
        // 验证服务质量
        let metrics = chaos_engine.get_metrics().await;
        assert!(metrics.availability > 0.99); // 99%可用性
        assert!(metrics.consistency_violations == 0);
    }
}
```

## 📈 性能基准测试

### 7.1 各模式性能对比
```rust
#[test_suite("performance_benchmarks")]
mod performance_benchmarks {
    use criterion::{black_box, Criterion};
    
    #[bench]
    fn bench_embedded_mode_throughput(c: &mut Criterion) {
        c.bench_function("embedded_insert_1k_vectors", |b| {
            b.iter(|| {
                let db = EmbeddedVectorDB::new().unwrap();
                for i in 0..1000 {
                    let vector = generate_test_vector(i);
                    black_box(db.insert_vector_sync(vector));
                }
            })
        });
    }
    
    #[bench]
    fn bench_cluster_mode_throughput(c: &mut Criterion) {
        c.bench_function("cluster_insert_1k_vectors", |b| {
            b.iter_batched(
                || TestCluster::new(ClusterType::ThreeNode),
                |cluster| {
                    tokio::runtime::Runtime::new().unwrap().block_on(async {
                        for i in 0..1000 {
                            let vector = generate_test_vector(i);
                            black_box(cluster.insert_vector(vector).await);
                        }
                    })
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    
    #[bench]
    fn bench_raft_consensus_latency(c: &mut Criterion) {
        c.bench_function("raft_log_replication", |b| {
            b.iter_batched(
                || TestCluster::new(ClusterType::ThreeNode),
                |cluster| {
                    tokio::runtime::Runtime::new().unwrap().block_on(async {
                        let leader = cluster.wait_for_leader().await;
                        let entry = b"test_log_entry".to_vec();
                        black_box(leader.propose_entry(entry).await);
                    })
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
}
```

## 🎯 成功标准

### 功能正确性
- [ ] 所有Raft算法测试通过
- [ ] 所有分片算法测试通过  
- [ ] 四种模式基本功能测试通过
- [ ] 故障恢复测试通过

### 性能指标
- [ ] 内嵌模式启动时间 < 1s
- [ ] 单机模式响应时间 < 10ms
- [ ] 3集群模式共识延迟 < 50ms
- [ ] 6集群模式吞吐量 > 10k qps

### 可靠性标准
- [ ] 99.9%可用性（在混沌测试中）
- [ ] 0数据丢失（在故障场景中）
- [ ] 100%最终一致性

## 📅 执行计划

### Phase 1: 基础测试框架 (Week 1)
- [ ] 实现TestCluster和TestNode基础设施
- [ ] 创建NetworkSimulator网络模拟器
- [ ] 搭建ChaosEngine混沌测试引擎

### Phase 2: 核心算法测试 (Week 2)
- [ ] 实现完整的Raft算法测试套件
- [ ] 实现分片算法测试套件
- [ ] 添加边界条件和异常场景测试

### Phase 3: 模式专项测试 (Week 3)
- [ ] 完成内嵌模式和单机模式测试
- [ ] 完成3节点和6节点集群测试
- [ ] 添加性能基准测试

### Phase 4: 集成和优化 (Week 4)
- [ ] 完成混沌工程测试
- [ ] 优化测试执行效率
- [ ] 完善测试文档和CI集成

## 🚀 持续集成

### CI/CD流水线
```yaml
# .github/workflows/comprehensive-tests.yml
name: 综合测试

on: [push, pull_request]

jobs:
  embedded-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: 运行内嵌模式测试
        run: cargo test --test embedded_tests
        
  standalone-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: 运行单机模式测试
        run: cargo test --test standalone_tests
        
  cluster-tests:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        cluster_size: [3, 6]
    steps:
      - uses: actions/checkout@v2
      - name: 运行集群测试
        run: cargo test --test cluster_tests -- --cluster-size ${{ matrix.cluster_size }}
        
  raft-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: 运行Raft算法测试
        run: cargo test --test raft_tests
        
  resharding-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: 运行分片算法测试
        run: cargo test --test resharding_tests
        
  chaos-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: 运行混沌工程测试
        run: cargo test --test chaos_tests
        timeout-minutes: 30
```

通过这个综合测试方案，我们将确保Grape Vector Database在所有运行模式下都能提供可靠、高性能的服务，特别是验证Raft共识算法和Resharding分片算法的正确性。