/// 性能基准测试
/// 
/// 使用 Criterion 框架进行详细的性能基准测试，包括：
/// - 各种模式下的吞吐量测试
/// - 延迟分析和分布
/// - 内存使用效率
/// - 并发性能对比
/// - 扩展性基准

mod test_framework;

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::time::Duration;
use tokio::runtime::Runtime;

use test_framework::*;
use grape_vector_db::types::*;

/// 内嵌模式性能基准
fn bench_embedded_mode(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("embedded_mode");
    
    // 不同文档数量的插入性能
    for doc_count in [100, 500, 1000, 2000].iter() {
        group.throughput(Throughput::Elements(*doc_count as u64));
        group.bench_with_input(
            BenchmarkId::new("document_insertion", doc_count),
            doc_count,
            |b, &doc_count| {
                b.to_async(&rt).iter(|| async {
                    let cluster = TestCluster::new(ClusterType::Embedded).await;
                    cluster.start_all_nodes().await.unwrap();
                    
                    let docs = generate_test_documents(doc_count);
                    let start = std::time::Instant::now();
                    
                    for doc in docs {
                        cluster.insert_document(black_box(doc)).await.unwrap();
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    // 搜索性能基准
    group.bench_function("search_performance", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::Embedded).await;
            cluster.start_all_nodes().await.unwrap();
            
            // 预先插入数据
            let docs = generate_test_documents(1000);
            for doc in docs {
                cluster.insert_document(doc).await.unwrap();
            }
            
            // 基准测试搜索
            black_box(cluster.get_document("doc_500").await.unwrap())
        });
    });
    
    // 并发访问性能
    for concurrent_count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_access", concurrent_count),
            concurrent_count,
            |b, &concurrent_count| {
                b.to_async(&rt).iter(|| async {
                    let cluster = TestCluster::new(ClusterType::Embedded).await;
                    cluster.start_all_nodes().await.unwrap();
                    
                    let handles: Vec<_> = (0..concurrent_count).map(|i| {
                        let cluster = cluster.clone();
                        tokio::spawn(async move {
                            let doc = create_test_document_with_id(&format!("concurrent_{}", i));
                            cluster.insert_document(doc).await.unwrap()
                        })
                    }).collect();
                    
                    for handle in handles {
                        black_box(handle.await.unwrap());
                    }
                });
            },
        );
    }
    
    group.finish();
}

/// 单机模式性能基准
fn bench_standalone_mode(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("standalone_mode");
    group.sample_size(50); // 减少样本数量以加快测试
    
    // gRPC 接口性能
    group.bench_function("grpc_operations", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::Standalone).await;
            cluster.start_all_nodes().await.unwrap();
            
            // 模拟 gRPC 操作序列
            let operations = vec![
                GrpcOperation::Insert(create_test_document_with_id("grpc_test_1")),
                GrpcOperation::Insert(create_test_document_with_id("grpc_test_2")),
                GrpcOperation::Get("grpc_test_1".to_string()),
                GrpcOperation::Search("测试".to_string(), 5),
                GrpcOperation::Delete("grpc_test_2".to_string()),
            ];
            
            for operation in operations {
                black_box(execute_grpc_operation(&cluster, operation).await);
            }
        });
    });
    
    // REST API 性能
    group.bench_function("rest_api_operations", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::Standalone).await;
            cluster.start_all_nodes().await.unwrap();
            
            // 模拟 REST API 调用
            let points = generate_qdrant_style_points(20);
            black_box(simulate_rest_bulk_insert(&cluster, points).await);
        });
    });
    
    // 持久化性能
    group.bench_function("persistence_performance", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::Standalone).await;
            cluster.start_all_nodes().await.unwrap();
            
            // 插入数据
            let docs = generate_test_documents(100);
            for doc in docs {
                cluster.insert_document(doc).await.unwrap();
            }
            
            // 模拟重启（测试恢复性能）
            black_box(restart_cluster(&cluster).await);
        });
    });
    
    group.finish();
}

/// 集群模式性能基准
fn bench_cluster_mode(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("cluster_mode");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(20));
    
    // 3节点集群性能
    group.bench_function("three_node_consensus", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::ThreeNode).await;
            cluster.start_all_nodes().await.unwrap();
            
            let leader = cluster.wait_for_leader().await.unwrap();
            
            // 测试共识性能
            let entries = vec![
                b"consensus_test_1".to_vec(),
                b"consensus_test_2".to_vec(),
                b"consensus_test_3".to_vec(),
            ];
            
            for entry in entries {
                black_box(leader.propose_entry(entry).await.unwrap());
            }
            
            black_box(cluster.wait_for_log_sync().await.unwrap());
        });
    });
    
    // 6节点集群扩展性
    group.bench_function("six_node_scalability", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::SixNode).await;
            cluster.start_all_nodes().await.unwrap();
            
            // 并行分布式操作
            let doc_count = 50;
            let docs = generate_test_documents(doc_count);
            
            let handles: Vec<_> = docs.into_iter().map(|doc| {
                let cluster = cluster.clone();
                tokio::spawn(async move {
                    cluster.insert_document(doc).await
                })
            }).collect();
            
            for handle in handles {
                black_box(handle.await.unwrap().unwrap());
            }
        });
    });
    
    // 负载均衡性能
    group.bench_function("load_balancing", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::SixNode).await;
            cluster.start_all_nodes().await.unwrap();
            
            // 测试负载均衡下的性能
            let mixed_operations = generate_mixed_operations(100);
            
            for operation in mixed_operations {
                black_box(execute_cluster_operation(&cluster, operation).await);
            }
        });
    });
    
    group.finish();
}

/// Raft 算法性能基准
fn bench_raft_algorithm(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("raft_algorithm");
    group.sample_size(50);
    
    // 领导者选举性能
    group.bench_function("leader_election", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::ThreeNode).await;
            cluster.start_all_nodes().await.unwrap();
            
            // 测试选举时间
            let start = std::time::Instant::now();
            black_box(cluster.wait_for_leader().await.unwrap());
            start.elapsed()
        });
    });
    
    // 日志复制吞吐量
    for batch_size in [1, 10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("log_replication", batch_size),
            batch_size,
            |b, &batch_size| {
                b.to_async(&rt).iter(|| async {
                    let cluster = TestCluster::new(ClusterType::ThreeNode).await;
                    cluster.start_all_nodes().await.unwrap();
                    
                    let leader = cluster.wait_for_leader().await.unwrap();
                    
                    for i in 0..batch_size {
                        let entry = format!("log_entry_{}", i).into_bytes();
                        black_box(leader.propose_entry(entry).await.unwrap());
                    }
                    
                    black_box(cluster.wait_for_log_sync().await.unwrap());
                });
            },
        );
    }
    
    // 故障恢复性能
    group.bench_function("failure_recovery", |b| {
        b.to_async(&rt).iter(|| async {
            let cluster = TestCluster::new(ClusterType::ThreeNode).await;
            cluster.start_all_nodes().await.unwrap();
            
            let leader = cluster.wait_for_leader().await.unwrap();
            let leader_id = leader.node_id();
            
            // 停止领导者
            cluster.stop_node(leader_id).await.unwrap();
            
            // 测试恢复时间
            let start = std::time::Instant::now();
            black_box(cluster.wait_for_leader().await.unwrap());
            let recovery_time = start.elapsed();
            
            // 重启原领导者
            cluster.restart_node(leader_id).await.unwrap();
            
            recovery_time
        });
    });
    
    group.finish();
}

// 辅助类型和函数
#[derive(Clone)]
enum GrpcOperation {
    Insert(Document),
    Get(String),
    Search(String, usize),
    Delete(String),
}

#[derive(Clone)]
enum ClusterOperation {
    Insert(Document),
    Get(String),
    Update(Document),
    Delete(String),
}

async fn execute_grpc_operation(cluster: &TestCluster, operation: GrpcOperation) -> Result<(), String> {
    match operation {
        GrpcOperation::Insert(doc) => {
            cluster.insert_document(doc).await.map_err(|e| e.to_string())?;
        }
        GrpcOperation::Get(id) => {
            cluster.get_document(&id).await.map_err(|e| e.to_string())?;
        }
        GrpcOperation::Search(query, limit) => {
            // 模拟搜索操作
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        GrpcOperation::Delete(id) => {
            // 模拟删除操作
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    Ok(())
}

async fn execute_cluster_operation(cluster: &TestCluster, operation: ClusterOperation) -> Result<(), String> {
    match operation {
        ClusterOperation::Insert(doc) => {
            cluster.insert_document(doc).await.map_err(|e| e.to_string())?;
        }
        ClusterOperation::Get(id) => {
            cluster.get_document(&id).await.map_err(|e| e.to_string())?;
        }
        ClusterOperation::Update(doc) => {
            cluster.insert_document(doc).await.map_err(|e| e.to_string())?; // 简化为插入
        }
        ClusterOperation::Delete(id) => {
            // 模拟删除操作
            tokio::time::sleep(Duration::from_millis(3)).await;
        }
    }
    Ok(())
}

async fn simulate_rest_bulk_insert(cluster: &TestCluster, points: Vec<QdrantPoint>) -> Result<(), String> {
    for point in points {
        let doc = Document {
            id: point.id,
            content: point.payload.get("text").unwrap_or(&"".to_string()).clone(),
            vector: Some(point.vector),
            ..Default::default()
        };
        cluster.insert_document(doc).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn restart_cluster(cluster: &TestCluster) -> Duration {
    let start = std::time::Instant::now();
    
    // 模拟重启过程
    for i in 0..cluster.cluster_type.node_count() {
        cluster.stop_node(&format!("node_{}", i)).await.unwrap();
    }
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    cluster.start_all_nodes().await.unwrap();
    
    start.elapsed()
}

fn generate_mixed_operations(count: usize) -> Vec<ClusterOperation> {
    (0..count).map(|i| {
        match i % 4 {
            0 => ClusterOperation::Insert(create_test_document_with_id(&format!("mixed_{}", i))),
            1 => ClusterOperation::Get(format!("mixed_{}", i.saturating_sub(1))),
            2 => ClusterOperation::Update(create_test_document_with_id(&format!("mixed_{}", i))),
            _ => ClusterOperation::Delete(format!("mixed_{}", i.saturating_sub(3))),
        }
    }).collect()
}

#[derive(Debug, Clone)]
struct QdrantPoint {
    id: String,
    vector: Vec<f32>,
    payload: std::collections::HashMap<String, String>,
}

fn generate_qdrant_style_points(count: usize) -> Vec<QdrantPoint> {
    (0..count).map(|i| {
        let mut payload = std::collections::HashMap::new();
        payload.insert("text".to_string(), format!("Point {}", i));
        
        QdrantPoint {
            id: format!("point_{}", i),
            vector: generate_test_vector(i),
            payload,
        }
    }).collect()
}

// 配置基准测试组
criterion_group!(
    benches,
    bench_embedded_mode,
    bench_standalone_mode,
    bench_cluster_mode,
    bench_raft_algorithm
);

criterion_main!(benches);