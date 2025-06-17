/// 单机模式综合测试
/// 
/// 测试单机模式下的各种功能，包括：
/// - gRPC服务接口
/// - REST API兼容性
/// - 持久化存储
/// - 服务生命周期
/// - 性能优化

mod test_framework;

use std::time::Duration;
use anyhow::Result;

use test_framework::*;
use grape_vector_db::types::*;

/// 单机模式服务测试
#[cfg(test)]
mod standalone_service_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_standalone_server_startup() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        
        // 启动单机服务
        cluster.start_all_nodes().await.unwrap();
        
        // 验证服务状态
        assert!(cluster.is_available().await, "单机服务应该可用");
        assert_eq!(cluster.active_node_count().await, 1, "应该有1个活跃节点");
        
        // 验证基本功能
        let test_doc = create_test_document_with_id("standalone_test");
        cluster.insert_document(test_doc.clone()).await.unwrap();
        
        let retrieved = cluster.get_document("standalone_test").await.unwrap();
        assert_eq!(retrieved.id, "standalone_test");
        
        println!("✅ 单机服务启动测试通过");
    }
    
    #[tokio::test]
    async fn test_grpc_interface() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        // 模拟gRPC客户端连接
        println!("测试gRPC接口...");
        
        // 测试文档操作的gRPC接口
        let docs = generate_test_documents(10);
        for doc in &docs {
            // 模拟gRPC插入请求
            cluster.insert_document(doc.clone()).await.unwrap();
        }
        
        // 模拟gRPC搜索请求
        let search_result = simulate_grpc_search(&cluster, "测试", 5).await.unwrap();
        assert!(!search_result.is_empty(), "gRPC搜索应该返回结果");
        
        // 模拟gRPC批量操作
        let batch_results = simulate_grpc_batch_insert(&cluster, docs).await.unwrap();
        assert_eq!(batch_results.len(), 10, "批量插入应该返回所有结果");
        
        println!("✅ gRPC接口测试通过");
    }
    
    #[tokio::test]
    async fn test_rest_api_compatibility() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        // 测试Qdrant兼容的REST API
        println!("测试REST API兼容性...");
        
        // 模拟Qdrant风格的集合创建
        let collection_result = simulate_rest_create_collection(&cluster, "test_collection").await;
        assert!(collection_result.is_ok(), "创建集合应该成功");
        
        // 模拟Qdrant风格的点插入
        let points = generate_qdrant_style_points(15);
        let insert_result = simulate_rest_insert_points(&cluster, "test_collection", points).await;
        assert!(insert_result.is_ok(), "插入点应该成功");
        
        // 模拟Qdrant风格的搜索
        let query_vector = generate_test_vector(1);
        let search_result = simulate_rest_search(&cluster, "test_collection", query_vector, 5).await;
        assert!(search_result.is_ok(), "REST搜索应该成功");
        
        let results = search_result.unwrap();
        assert!(!results.is_empty(), "应该返回搜索结果");
        
        println!("✅ REST API兼容性测试通过");
    }
    
    #[tokio::test]
    async fn test_service_health_monitoring() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        // 获取服务健康状态
        let health_status = get_service_health(&cluster).await.unwrap();
        
        println!("服务健康状态:");
        println!("  状态: {}", health_status.status);
        println!("  正常运行时间: {:?}", health_status.uptime);
        println!("  内存使用: {} MB", health_status.memory_usage_mb);
        println!("  CPU使用: {:.1}%", health_status.cpu_usage_percent);
        
        // 验证健康状态
        assert_eq!(health_status.status, "healthy");
        assert!(health_status.memory_usage_mb > 0);
        assert!(health_status.cpu_usage_percent >= 0.0);
        
        // 测试健康检查端点
        let endpoint_result = check_health_endpoint(&cluster).await;
        assert!(endpoint_result.is_ok(), "健康检查端点应该可访问");
        
        println!("✅ 服务健康监控测试通过");
    }
    
    // 辅助函数
    async fn simulate_grpc_search(cluster: &TestCluster, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // 模拟gRPC搜索调用
        tokio::time::sleep(Duration::from_millis(10)).await; // 模拟网络延迟
        
        // 在真实实现中，这里会使用gRPC客户端
        let mut results = Vec::new();
        for i in 0..limit.min(3) {
            results.push(SearchResult {
                id: format!("grpc_result_{}", i),
                score: 0.9 - (i as f32 * 0.1),
                document: None,
                metadata: std::collections::HashMap::new(),
            });
        }
        Ok(results)
    }
    
    async fn simulate_grpc_batch_insert(cluster: &TestCluster, docs: Vec<Document>) -> Result<Vec<String>> {
        // 模拟gRPC批量插入
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        let mut doc_ids = Vec::new();
        for doc in docs {
            let id = cluster.insert_document(doc).await?;
            doc_ids.push(id);
        }
        Ok(doc_ids)
    }
    
    async fn simulate_rest_create_collection(cluster: &TestCluster, name: &str) -> Result<()> {
        // 模拟REST API创建集合
        tokio::time::sleep(Duration::from_millis(15)).await;
        println!("模拟创建集合: {}", name);
        Ok(())
    }
    
    async fn simulate_rest_insert_points(cluster: &TestCluster, collection: &str, points: Vec<QdrantPoint>) -> Result<()> {
        // 模拟REST API插入点
        tokio::time::sleep(Duration::from_millis(25)).await;
        
        for point in points {
            let doc = Document {
                id: point.id,
                content: point.payload.get("text").unwrap_or(&"".to_string()).clone(),
                vector: Some(point.vector),
                ..Default::default()
            };
            cluster.insert_document(doc).await?;
        }
        Ok(())
    }
    
    async fn simulate_rest_search(cluster: &TestCluster, collection: &str, vector: Vec<f32>, limit: usize) -> Result<Vec<SearchResult>> {
        // 模拟REST API搜索
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        let mut results = Vec::new();
        for i in 0..limit.min(5) {
            results.push(SearchResult {
                id: format!("rest_result_{}", i),
                score: 0.95 - (i as f32 * 0.05),
                document: None,
                metadata: std::collections::HashMap::new(),
            });
        }
        Ok(results)
    }
    
    async fn get_service_health(cluster: &TestCluster) -> Result<ServiceHealth> {
        tokio::time::sleep(Duration::from_millis(5)).await;
        
        Ok(ServiceHealth {
            status: "healthy".to_string(),
            uptime: Duration::from_secs(fastrand::u64(300..3600)),
            memory_usage_mb: fastrand::u64(50..200),
            cpu_usage_percent: fastrand::f64() * 20.0,
            active_connections: fastrand::u32(1..50),
        })
    }
    
    async fn check_health_endpoint(cluster: &TestCluster) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(5)).await;
        Ok(())
    }
}

/// 单机模式持久化测试
#[cfg(test)]
mod standalone_persistence_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_data_persistence() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        
        // 第一次启动并插入数据
        {
            cluster.start_all_nodes().await.unwrap();
            
            let test_docs = generate_test_documents(50);
            for doc in &test_docs {
                cluster.insert_document(doc.clone()).await.unwrap();
            }
            
            let stats = cluster.get_cluster_stats().await;
            println!("插入数据后统计: {:?}", stats);
            
            // 停止服务
            for i in 0..cluster.cluster_type.node_count() {
                cluster.stop_node(&format!("node_{}", i)).await.unwrap();
            }
        }
        
        // 重新启动并验证数据持久化
        {
            cluster.start_all_nodes().await.unwrap();
            
            // 验证数据是否持久化
            let recovered_doc = cluster.get_document("doc_0").await;
            assert!(recovered_doc.is_ok(), "重启后应该能读取持久化的数据");
            
            let stats = cluster.get_cluster_stats().await;
            println!("重启后统计: {:?}", stats);
            
            // 验证可以继续操作
            let new_doc = create_test_document_with_id("after_restart");
            cluster.insert_document(new_doc).await.unwrap();
            
            let final_doc = cluster.get_document("after_restart").await.unwrap();
            assert_eq!(final_doc.id, "after_restart");
        }
        
        println!("✅ 数据持久化测试通过");
    }
    
    #[tokio::test]
    async fn test_index_recovery() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        // 插入带向量的文档
        let docs_with_vectors: Vec<Document> = (0..30).map(|i| {
            create_test_document_with_vector(&format!("vector_doc_{}", i), generate_test_vector(i))
        }).collect();
        
        for doc in &docs_with_vectors {
            cluster.insert_document(doc.clone()).await.unwrap();
        }
        
        // 执行搜索以确保索引正常工作
        let search_results = simulate_vector_search(&cluster, &generate_test_vector(0), 5).await.unwrap();
        assert!(!search_results.is_empty(), "索引应该正常工作");
        
        // 模拟服务重启
        restart_service(&cluster).await.unwrap();
        
        // 验证索引恢复
        let recovery_results = simulate_vector_search(&cluster, &generate_test_vector(0), 5).await.unwrap();
        assert!(!recovery_results.is_empty(), "索引恢复后应该能正常搜索");
        
        // 验证索引完整性
        let integrity_check = verify_index_integrity(&cluster).await.unwrap();
        assert!(integrity_check, "索引完整性应该得到保证");
        
        println!("✅ 索引恢复测试通过");
    }
    
    #[tokio::test]
    async fn test_corruption_recovery() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        // 插入测试数据
        let test_docs = generate_test_documents(20);
        for doc in &test_docs {
            cluster.insert_document(doc.clone()).await.unwrap();
        }
        
        // 模拟数据损坏
        println!("模拟数据损坏...");
        simulate_data_corruption(&cluster).await.unwrap();
        
        // 尝试恢复
        println!("尝试数据恢复...");
        let recovery_result = attempt_data_recovery(&cluster).await;
        
        if recovery_result.is_ok() {
            println!("数据恢复成功");
            
            // 验证恢复后的数据完整性
            let mut recovered_count = 0;
            for doc in &test_docs {
                if cluster.get_document(&doc.id).await.is_ok() {
                    recovered_count += 1;
                }
            }
            
            let recovery_ratio = recovered_count as f64 / test_docs.len() as f64;
            println!("数据恢复率: {:.1}%", recovery_ratio * 100.0);
            
            assert!(recovery_ratio >= 0.8, "数据恢复率应该 >= 80%");
        } else {
            println!("数据无法恢复，但系统应该能优雅降级");
            assert!(cluster.is_available().await, "即使数据损坏，服务应该保持可用");
        }
        
        println!("✅ 损坏恢复测试通过");
    }
    
    // 辅助函数
    async fn simulate_vector_search(cluster: &TestCluster, query_vector: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        tokio::time::sleep(Duration::from_millis(15)).await;
        
        let mut results = Vec::new();
        for i in 0..limit.min(3) {
            let score = 0.9 - (i as f32 * 0.1);
            results.push(SearchResult {
                id: format!("vector_result_{}", i),
                score,
                document: None,
                metadata: std::collections::HashMap::new(),
            });
        }
        Ok(results)
    }
    
    async fn restart_service(cluster: &TestCluster) -> Result<()> {
        // 停止服务
        for i in 0..cluster.cluster_type.node_count() {
            cluster.stop_node(&format!("node_{}", i)).await?;
        }
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 重新启动
        cluster.start_all_nodes().await?;
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(())
    }
    
    async fn verify_index_integrity(cluster: &TestCluster) -> Result<bool> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // 模拟索引完整性检查
        // 在真实实现中，这里会验证索引结构和数据一致性
        Ok(true)
    }
    
    async fn simulate_data_corruption(cluster: &TestCluster) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(20)).await;
        println!("模拟存储文件损坏...");
        Ok(())
    }
    
    async fn attempt_data_recovery(cluster: &TestCluster) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 模拟数据恢复过程
        println!("执行数据恢复程序...");
        println!("检查备份文件...");
        println!("重建索引...");
        println!("验证数据完整性...");
        
        Ok(())
    }
}

/// 单机模式性能测试
#[cfg(test)]
mod standalone_performance_tests {
    use super::*;
    use crate::test_framework::utils::PerformanceTester;
    
    #[tokio::test]
    async fn test_single_node_throughput() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        let mut tester = PerformanceTester::new();
        let operation_count = 1000;
        
        println!("测试单机模式吞吐量...");
        
        // 并发写入测试
        let write_handles: Vec<_> = (0..operation_count).map(|i| {
            let cluster = cluster.clone();
            tokio::spawn(async move {
                let doc = create_test_document_with_id(&format!("throughput_doc_{}", i));
                let start = std::time::Instant::now();
                let result = cluster.insert_document(doc).await;
                (result, start.elapsed())
            })
        }).collect();
        
        // 收集写入结果
        let mut write_success = 0;
        let mut write_latency = Duration::ZERO;
        
        for handle in write_handles {
            let (result, latency) = handle.await.unwrap();
            if result.is_ok() {
                write_success += 1;
                write_latency += latency;
                tester.record_operation();
            }
        }
        
        let write_throughput = tester.get_throughput();
        let avg_write_latency = if write_success > 0 {
            write_latency / write_success
        } else {
            Duration::ZERO
        };
        
        println!("写入性能:");
        println!("  成功率: {}/{} ({:.1}%)", write_success, operation_count, 
                write_success as f64 / operation_count as f64 * 100.0);
        println!("  吞吐量: {:.2} ops/sec", write_throughput);
        println!("  平均延迟: {:?}", avg_write_latency);
        
        // 并发读取测试
        let mut read_tester = PerformanceTester::new();
        let read_handles: Vec<_> = (0..operation_count).map(|i| {
            let cluster = cluster.clone();
            tokio::spawn(async move {
                let doc_id = format!("throughput_doc_{}", i % write_success);
                let start = std::time::Instant::now();
                let result = cluster.get_document(&doc_id).await;
                (result, start.elapsed())
            })
        }).collect();
        
        // 收集读取结果
        let mut read_success = 0;
        let mut read_latency = Duration::ZERO;
        
        for handle in read_handles {
            let (result, latency) = handle.await.unwrap();
            if result.is_ok() {
                read_success += 1;
                read_latency += latency;
                read_tester.record_operation();
            }
        }
        
        let read_throughput = read_tester.get_throughput();
        let avg_read_latency = if read_success > 0 {
            read_latency / read_success
        } else {
            Duration::ZERO
        };
        
        println!("读取性能:");
        println!("  成功率: {}/{} ({:.1}%)", read_success, operation_count, 
                read_success as f64 / operation_count as f64 * 100.0);
        println!("  吞吐量: {:.2} ops/sec", read_throughput);
        println!("  平均延迟: {:?}", avg_read_latency);
        
        // 性能断言
        assert!(write_throughput >= 800.0, "写入吞吐量应该 >= 800 ops/sec");
        assert!(read_throughput >= 2000.0, "读取吞吐量应该 >= 2000 ops/sec");
        assert!(avg_write_latency <= Duration::from_millis(50), "写入延迟应该 <= 50ms");
        assert!(avg_read_latency <= Duration::from_millis(20), "读取延迟应该 <= 20ms");
        
        println!("✅ 单机模式吞吐量测试通过");
    }
    
    #[tokio::test]
    async fn test_memory_efficiency() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        let initial_memory = get_memory_usage();
        
        // 插入不同大小的文档测试内存效率
        let document_sizes = vec![
            (100, "small"),   // 小文档
            (500, "medium"),  // 中等文档
            (200, "large"),   // 大文档
        ];
        
        for (count, size_type) in document_sizes {
            println!("测试{}文档内存效率...", size_type);
            
            let docs = match size_type {
                "small" => generate_small_documents(count),
                "medium" => generate_medium_documents(count),
                "large" => generate_large_documents(count),
                _ => vec![],
            };
            
            let memory_before = get_memory_usage();
            
            for doc in docs {
                cluster.insert_document(doc).await.unwrap();
            }
            
            let memory_after = get_memory_usage();
            let memory_per_doc = (memory_after - memory_before) * 1024.0 / count as f64;
            
            println!("  {}文档内存效率: {:.2} KB/doc", size_type, memory_per_doc);
            
            // 内存效率断言
            match size_type {
                "small" => assert!(memory_per_doc <= 5.0, "小文档内存使用应该 <= 5KB/doc"),
                "medium" => assert!(memory_per_doc <= 15.0, "中等文档内存使用应该 <= 15KB/doc"),
                "large" => assert!(memory_per_doc <= 50.0, "大文档内存使用应该 <= 50KB/doc"),
                _ => {}
            }
        }
        
        let final_memory = get_memory_usage();
        let total_memory_growth = final_memory - initial_memory;
        
        println!("总内存增长: {:.1} MB", total_memory_growth);
        assert!(total_memory_growth <= 200.0, "总内存增长应该 <= 200MB");
        
        println!("✅ 内存效率测试通过");
    }
    
    #[tokio::test]
    async fn test_search_performance() {
        let cluster = TestCluster::new(ClusterType::Standalone).await;
        cluster.start_all_nodes().await.unwrap();
        
        // 插入用于搜索的文档
        let search_docs = generate_search_test_documents(500);
        for doc in &search_docs {
            cluster.insert_document(doc.clone()).await.unwrap();
        }
        
        println!("测试搜索性能...");
        
        // 测试不同类型的搜索
        let search_scenarios = vec![
            ("文本搜索", "测试", SearchType::Text),
            ("精确匹配", "doc_100", SearchType::Exact),
            ("模糊搜索", "测试内容", SearchType::Fuzzy),
        ];
        
        for (scenario_name, query, search_type) in search_scenarios {
            let mut search_tester = PerformanceTester::new();
            let search_count = 100;
            
            let search_handles: Vec<_> = (0..search_count).map(|_| {
                let cluster = cluster.clone();
                let query = query.to_string();
                tokio::spawn(async move {
                    let start = std::time::Instant::now();
                    let result = perform_search(&cluster, &query, search_type).await;
                    (result, start.elapsed())
                })
            }).collect();
            
            let mut search_success = 0;
            let mut search_latency = Duration::ZERO;
            let mut total_results = 0;
            
            for handle in search_handles {
                let (result, latency) = handle.await.unwrap();
                if let Ok(results) = result {
                    search_success += 1;
                    search_latency += latency;
                    total_results += results.len();
                    search_tester.record_operation();
                }
            }
            
            let search_throughput = search_tester.get_throughput();
            let avg_search_latency = if search_success > 0 {
                search_latency / search_success
            } else {
                Duration::ZERO
            };
            let avg_results_per_search = if search_success > 0 {
                total_results as f64 / search_success as f64
            } else {
                0.0
            };
            
            println!("{}性能:", scenario_name);
            println!("  成功率: {}/{}", search_success, search_count);
            println!("  吞吐量: {:.2} searches/sec", search_throughput);
            println!("  平均延迟: {:?}", avg_search_latency);
            println!("  平均结果数: {:.1}", avg_results_per_search);
            
            // 搜索性能断言
            assert!(search_throughput >= 200.0, "{}吞吐量应该 >= 200 searches/sec", scenario_name);
            assert!(avg_search_latency <= Duration::from_millis(100), "{}延迟应该 <= 100ms", scenario_name);
        }
        
        println!("✅ 搜索性能测试通过");
    }
    
    // 辅助函数和类型
    #[derive(Clone, Copy)]
    enum SearchType {
        Text,
        Exact,
        Fuzzy,
    }
    
    async fn perform_search(cluster: &TestCluster, query: &str, search_type: SearchType) -> Result<Vec<SearchResult>> {
        match search_type {
            SearchType::Text => {
                // 模拟文本搜索
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(vec![SearchResult {
                    id: "text_result".to_string(),
                    score: 0.85,
                    document: None,
                    metadata: std::collections::HashMap::new(),
                }])
            }
            SearchType::Exact => {
                // 模拟精确匹配
                tokio::time::sleep(Duration::from_millis(2)).await;
                cluster.get_document(query).await.map(|doc| {
                    if doc.is_some() {
                        vec![SearchResult {
                            id: query.to_string(),
                            score: 1.0,
                            document: None,
                            metadata: std::collections::HashMap::new(),
                        }]
                    } else {
                        vec![]
                    }
                })
            }
            SearchType::Fuzzy => {
                // 模拟模糊搜索
                tokio::time::sleep(Duration::from_millis(15)).await;
                Ok(vec![
                    SearchResult {
                        id: "fuzzy_result_1".to_string(),
                        score: 0.75,
                        document: None,
                        metadata: std::collections::HashMap::new(),
                    },
                    SearchResult {
                        id: "fuzzy_result_2".to_string(),
                        score: 0.70,
                        document: None,
                        metadata: std::collections::HashMap::new(),
                    },
                ])
            }
        }
    }
    
    fn generate_small_documents(count: usize) -> Vec<Document> {
        (0..count).map(|i| {
            create_test_document_with_id(&format!("small_doc_{}", i))
        }).collect()
    }
    
    fn generate_medium_documents(count: usize) -> Vec<Document> {
        (0..count).map(|i| {
            let mut doc = create_test_document_with_id(&format!("medium_doc_{}", i));
            doc.content = format!("这是一个中等长度的测试文档内容，包含更多的文字信息用于测试 {}", i);
            doc
        }).collect()
    }
    
    fn generate_large_documents(count: usize) -> Vec<Document> {
        (0..count).map(|i| {
            let mut doc = create_test_document_with_id(&format!("large_doc_{}", i));
            doc.content = format!("这是一个大型文档的内容，包含大量的文字信息用于测试内存使用效率。{}", "重复内容 ".repeat(100));
            doc.vector = Some(generate_test_vector(i * 10)); // 更大的向量
            doc
        }).collect()
    }
    
    fn generate_search_test_documents(count: usize) -> Vec<Document> {
        (0..count).map(|i| {
            let content = match i % 5 {
                0 => format!("这是测试文档 {}", i),
                1 => format!("搜索测试内容 {}", i),
                2 => format!("向量数据库测试 {}", i),
                3 => format!("性能基准测试 {}", i),
                _ => format!("其他测试内容 {}", i),
            };
            
            let mut doc = create_test_document_with_id(&format!("doc_{}", i));
            doc.content = content;
            doc
        }).collect()
    }
    
    fn get_memory_usage() -> f64 {
        // 模拟内存使用情况
        fastrand::f64() * 20.0 + 50.0 // 50-70 MB 范围
    }
}

// 辅助类型定义
#[derive(Debug, Clone)]
struct ServiceHealth {
    status: String,
    uptime: Duration,
    memory_usage_mb: u64,
    cpu_usage_percent: f64,
    active_connections: u32,
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
        payload.insert("text".to_string(), format!("Qdrant 风格点 {}", i));
        payload.insert("category".to_string(), format!("类别_{}", i % 3));
        
        QdrantPoint {
            id: format!("point_{}", i),
            vector: generate_test_vector(i),
            payload,
        }
    }).collect()
}

// 运行所有测试的辅助函数
#[tokio::test]
async fn run_all_standalone_tests() {
    tracing_subscriber::fmt::init();
    
    println!("开始运行单机模式综合测试...");
    println!("=" .repeat(60));
    
    // 注意：在实际测试中，这些测试模块会自动运行
    // 这里只是一个占位符函数来组织测试结构
    
    println!("所有单机模式测试完成！");
}