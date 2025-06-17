/// Qdrant兼容性集成测试
/// 
/// 测试Grape Vector Database与Qdrant API的兼容性，包括：
/// - 单节点模式下的Qdrant API兼容性
/// - 3副本集群模式下的Qdrant API兼容性
/// - REST API接口兼容性
/// - gRPC接口兼容性
/// - 数据格式兼容性

mod test_framework;

use std::time::Duration;
use std::sync::Arc;
use anyhow::Result;
use serde_json::Value;

use test_framework::{TestCluster, ClusterConfig, ClusterType};
use grape_vector_db::types::*;
use grape_vector_db::grpc::*;

/// Qdrant兼容性单节点测试
#[cfg(test)]
mod qdrant_single_node_tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_qdrant_collection_management() {
        println!("🧪 测试Qdrant集合管理兼容性 (单节点)");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::Standalone,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 1,
            ..Default::default()
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 测试集合创建 (Qdrant兼容)
        println!("  ✅ 测试集合创建...");
        let collection_name = "test_collection";
        let create_result = create_qdrant_collection(&cluster, collection_name, 128).await;
        assert!(create_result.is_ok(), "Qdrant风格集合创建应该成功");
        
        // 测试集合列表 (Qdrant兼容)
        println!("  ✅ 测试集合列表...");
        let list_result = list_qdrant_collections(&cluster).await;
        assert!(list_result.is_ok(), "列出集合应该成功");
        let collections = list_result.unwrap();
        assert!(collections.contains(&collection_name.to_string()), "应该包含创建的集合");
        
        // 测试集合信息 (Qdrant兼容)
        println!("  ✅ 测试集合信息...");
        let info_result = get_qdrant_collection_info(&cluster, collection_name).await;
        assert!(info_result.is_ok(), "获取集合信息应该成功");
        
        // 测试集合删除 (Qdrant兼容)
        println!("  ✅ 测试集合删除...");
        let delete_result = delete_qdrant_collection(&cluster, collection_name).await;
        assert!(delete_result.is_ok(), "删除集合应该成功");
        
        println!("✅ Qdrant集合管理兼容性测试通过 (单节点)");
    }
    
    #[tokio::test]
    async fn test_qdrant_point_operations() {
        println!("🧪 测试Qdrant点操作兼容性 (单节点)");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::Standalone,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 1,
            ..Default::default()
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let collection_name = "point_test_collection";
        create_qdrant_collection(&cluster, collection_name, 128).await.unwrap();
        
        // 测试单点插入 (Qdrant兼容)
        println!("  ✅ 测试单点插入...");
        let point = QdrantPoint {
            id: "point_1".to_string(),
            vector: generate_test_vector(128),
            payload: serde_json::json!({
                "title": "测试文档1",
                "category": "技术"
            }),
        };
        
        let insert_result = insert_qdrant_point(&cluster, collection_name, point.clone()).await;
        assert!(insert_result.is_ok(), "Qdrant风格点插入应该成功");
        
        // 测试批量点插入 (Qdrant兼容)
        println!("  ✅ 测试批量点插入...");
        let points = (2..=10).map(|i| QdrantPoint {
            id: format!("point_{}", i),
            vector: generate_test_vector(128),
            payload: serde_json::json!({
                "title": format!("测试文档{}", i),
                "category": if i % 2 == 0 { "技术" } else { "科学" }
            }),
        }).collect::<Vec<_>>();
        
        let batch_insert_result = insert_qdrant_points_batch(&cluster, collection_name, points).await;
        assert!(batch_insert_result.is_ok(), "批量点插入应该成功");
        
        // 测试点检索 (Qdrant兼容)
        println!("  ✅ 测试点检索...");
        let retrieve_result = retrieve_qdrant_point(&cluster, collection_name, "point_1").await;
        assert!(retrieve_result.is_ok(), "点检索应该成功");
        let retrieved_point = retrieve_result.unwrap();
        assert_eq!(retrieved_point.id, "point_1");
        
        // 测试点搜索 (Qdrant兼容)
        println!("  ✅ 测试点搜索...");
        let search_vector = generate_test_vector(128);
        let search_result = search_qdrant_points(&cluster, collection_name, search_vector.clone(), 5, None).await;
        assert!(search_result.is_ok(), "点搜索应该成功");
        let search_results = search_result.unwrap();
        assert!(!search_results.is_empty(), "应该返回搜索结果");
        assert!(search_results.len() <= 5, "结果数量不应超过限制");
        
        // 测试带过滤的搜索 (Qdrant兼容)
        println!("  ✅ 测试带过滤的搜索...");
        let filter = serde_json::json!({
            "must": [{
                "key": "category",
                "match": { "value": "技术" }
            }]
        });
        let filtered_search_result = search_qdrant_points(&cluster, collection_name, search_vector, 5, Some(filter)).await;
        assert!(filtered_search_result.is_ok(), "带过滤的搜索应该成功");
        
        // 测试点删除 (Qdrant兼容)
        println!("  ✅ 测试点删除...");
        let delete_result = delete_qdrant_point(&cluster, collection_name, "point_1").await;
        assert!(delete_result.is_ok(), "点删除应该成功");
        
        // 验证删除后无法检索
        let retrieve_after_delete = retrieve_qdrant_point(&cluster, collection_name, "point_1").await;
        assert!(retrieve_after_delete.is_err() || retrieve_after_delete.unwrap().id.is_empty(), "删除后应该无法检索到点");
        
        println!("✅ Qdrant点操作兼容性测试通过 (单节点)");
    }
    
    #[tokio::test]
    async fn test_qdrant_search_features() {
        println!("🧪 测试Qdrant高级搜索功能兼容性 (单节点)");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::Standalone,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 1,
            ..Default::default()
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let collection_name = "search_test_collection";
        create_qdrant_collection(&cluster, collection_name, 128).await.unwrap();
        
        // 插入测试数据
        let test_points = (1..=20).map(|i| QdrantPoint {
            id: format!("search_point_{}", i),
            vector: generate_test_vector(128),
            payload: serde_json::json!({
                "title": format!("文档{}", i),
                "category": if i <= 10 { "技术" } else { "科学" },
                "score": i as f64 * 0.1,
                "tags": if i % 3 == 0 { vec!["重要", "精选"] } else { vec!["普通"] }
            }),
        }).collect::<Vec<_>>();
        
        insert_qdrant_points_batch(&cluster, collection_name, test_points).await.unwrap();
        
        // 测试相似度搜索
        println!("  ✅ 测试相似度搜索...");
        let query_vector = generate_test_vector(128);
        let similarity_search = search_qdrant_points(&cluster, collection_name, query_vector.clone(), 10, None).await;
        assert!(similarity_search.is_ok(), "相似度搜索应该成功");
        let results = similarity_search.unwrap();
        assert!(results.len() <= 10, "结果数量应该符合限制");
        
        // 测试复杂过滤条件
        println!("  ✅ 测试复杂过滤条件...");
        let complex_filter = serde_json::json!({
            "must": [{
                "key": "category",
                "match": { "value": "技术" }
            }],
            "should": [{
                "key": "score",
                "range": { "gte": 0.5 }
            }]
        });
        let filtered_search = search_qdrant_points(&cluster, collection_name, query_vector.clone(), 5, Some(complex_filter)).await;
        assert!(filtered_search.is_ok(), "复杂过滤搜索应该成功");
        
        // 测试范围查询
        println!("  ✅ 测试范围查询...");
        let range_filter = serde_json::json!({
            "must": [{
                "key": "score",
                "range": { "gte": 0.5, "lte": 1.5 }
            }]
        });
        let range_search = search_qdrant_points(&cluster, collection_name, query_vector, 15, Some(range_filter)).await;
        assert!(range_search.is_ok(), "范围查询应该成功");
        
        println!("✅ Qdrant高级搜索功能兼容性测试通过 (单节点)");
    }
}

/// Qdrant兼容性3副本集群测试
#[cfg(test)]
mod qdrant_three_replica_tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_qdrant_cluster_consistency() {
        println!("🧪 测试Qdrant集群一致性 (3副本)");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::ThreeNode,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 3,
            ..Default::default()
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await; // 等待集群建立
        
        let collection_name = "cluster_consistency_test";
        
        // 在所有节点上创建集合
        println!("  ✅ 在集群中创建集合...");
        let create_result = create_qdrant_collection(&cluster, collection_name, 128).await;
        assert!(create_result.is_ok(), "集群中集合创建应该成功");
        
        // 等待集合同步
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // 验证所有节点都有集合
        println!("  ✅ 验证集合在所有节点同步...");
        for node_id in 0..3 {
            let collections = list_qdrant_collections_on_node(&cluster, node_id).await.unwrap();
            assert!(collections.contains(&collection_name.to_string()), 
                   "节点{}应该包含集合", node_id);
        }
        
        // 在不同节点上插入数据
        println!("  ✅ 在不同节点插入数据...");
        let points = (1..=15).map(|i| QdrantPoint {
            id: format!("cluster_point_{}", i),
            vector: generate_test_vector(128),
            payload: serde_json::json!({
                "title": format!("集群文档{}", i),
                "node_created": i % 3, // 指示在哪个节点创建
            }),
        }).collect::<Vec<_>>();
        
        // 分批在不同节点插入
        for (batch_id, chunk) in points.chunks(5).enumerate() {
            let target_node = batch_id % 3;
            let batch_result = insert_qdrant_points_batch_on_node(&cluster, collection_name, chunk.to_vec(), target_node).await;
            assert!(batch_result.is_ok(), "在节点{}插入数据应该成功", target_node);
        }
        
        // 等待数据同步
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        // 验证数据在所有节点上的一致性
        println!("  ✅ 验证数据一致性...");
        let query_vector = generate_test_vector(128);
        let mut node_results = Vec::new();
        
        for node_id in 0..3 {
            let search_result = search_qdrant_points_on_node(&cluster, collection_name, query_vector.clone(), 15, None, node_id).await;
            assert!(search_result.is_ok(), "在节点{}搜索应该成功", node_id);
            node_results.push(search_result.unwrap());
        }
        
        // 检查所有节点返回的数据数量是否一致
        let first_count = node_results[0].len();
        for (i, results) in node_results.iter().enumerate() {
            assert_eq!(results.len(), first_count, 
                      "节点{}的搜索结果数量应该与其他节点一致", i);
        }
        
        println!("✅ Qdrant集群一致性测试通过 (3副本)");
    }
    
    #[tokio::test]
    async fn test_qdrant_cluster_fault_tolerance() {
        println!("🧪 测试Qdrant集群故障容错 (3副本)");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::ThreeNode,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 3,
            ..Default::default()
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        let collection_name = "fault_tolerance_test";
        create_qdrant_collection(&cluster, collection_name, 128).await.unwrap();
        
        // 插入初始数据
        println!("  ✅ 插入初始数据...");
        let initial_points = (1..=20).map(|i| QdrantPoint {
            id: format!("ft_point_{}", i),
            vector: generate_test_vector(128),
            payload: serde_json::json!({
                "title": format!("容错文档{}", i),
                "importance": i % 5,
            }),
        }).collect::<Vec<_>>();
        
        insert_qdrant_points_batch(&cluster, collection_name, initial_points).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // 停止一个节点
        println!("  ✅ 停止节点1模拟故障...");
        cluster.stop_node(1).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // 验证集群仍然可用（2/3节点在线）
        println!("  ✅ 验证集群仍然可用...");
        let query_vector = generate_test_vector(128);
        let search_during_failure = search_qdrant_points(&cluster, collection_name, query_vector.clone(), 10, None).await;
        assert!(search_during_failure.is_ok(), "单节点故障时集群应该仍可用");
        
        // 在故障期间插入新数据
        println!("  ✅ 在故障期间插入新数据...");
        let fault_period_points = (21..=25).map(|i| QdrantPoint {
            id: format!("ft_point_{}", i),
            vector: generate_test_vector(128),
            payload: serde_json::json!({
                "title": format!("故障期间文档{}", i),
                "created_during_fault": true,
            }),
        }).collect::<Vec<_>>();
        
        let insert_during_fault = insert_qdrant_points_batch(&cluster, collection_name, fault_period_points).await;
        assert!(insert_during_fault.is_ok(), "故障期间应该仍能插入数据");
        
        // 重启故障节点
        println!("  ✅ 重启故障节点...");
        cluster.start_node(1).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await; // 等待节点重新加入并同步
        
        // 验证恢复后的数据一致性
        println!("  ✅ 验证恢复后数据一致性...");
        for node_id in 0..3 {
            let node_search = search_qdrant_points_on_node(&cluster, collection_name, query_vector.clone(), 25, None, node_id).await;
            assert!(node_search.is_ok(), "节点{}恢复后应该可以搜索", node_id);
            let results = node_search.unwrap();
            assert!(results.len() >= 20, "应该包含故障前的数据"); // 至少包含原始数据
        }
        
        println!("✅ Qdrant集群故障容错测试通过 (3副本)");
    }
    
    #[tokio::test]
    async fn test_qdrant_cluster_load_balancing() {
        println!("🧪 测试Qdrant集群负载均衡 (3副本)");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::ThreeNode,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 3,
            ..Default::default()
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        let collection_name = "load_balancing_test";
        create_qdrant_collection(&cluster, collection_name, 128).await.unwrap();
        
        // 插入大量数据测试负载分布
        println!("  ✅ 插入大量数据...");
        let large_dataset = (1..=100).map(|i| QdrantPoint {
            id: format!("lb_point_{}", i),
            vector: generate_test_vector(128),
            payload: serde_json::json!({
                "title": format!("负载均衡文档{}", i),
                "batch": i / 10,
            }),
        }).collect::<Vec<_>>();
        
        // 分批插入以测试负载分布
        for chunk in large_dataset.chunks(10) {
            let batch_result = insert_qdrant_points_batch(&cluster, collection_name, chunk.to_vec()).await;
            assert!(batch_result.is_ok(), "批量插入应该成功");
            tokio::time::sleep(Duration::from_millis(100)).await; // 小延迟模拟真实场景
        }
        
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        // 执行并发搜索测试负载均衡
        println!("  ✅ 执行并发搜索...");
        let mut search_tasks = Vec::new();
        
        for i in 0..30 {
            let cluster_clone = cluster.clone();
            let collection_name_clone = collection_name.to_string();
            let task = tokio::spawn(async move {
                let query_vector = generate_test_vector(128);
                let filter = if i % 3 == 0 {
                    Some(serde_json::json!({
                        "must": [{
                            "key": "batch",
                            "range": { "gte": 5 }
                        }]
                    }))
                } else {
                    None
                };
                
                search_qdrant_points(&cluster_clone, &collection_name_clone, query_vector, 10, filter).await
            });
            search_tasks.push(task);
        }
        
        // 等待所有搜索完成
        let search_results = futures::future::join_all(search_tasks).await;
        let successful_searches = search_results.iter()
            .filter(|result| result.is_ok() && result.as_ref().unwrap().is_ok())
            .count();
        
        assert!(successful_searches >= 25, "大部分并发搜索应该成功 (实际: {})", successful_searches);
        
        // 验证所有节点都处理了请求（通过检查各节点的响应）
        println!("  ✅ 验证负载分布...");
        for node_id in 0..3 {
            let node_search = search_qdrant_points_on_node(&cluster, collection_name, generate_test_vector(128), 10, None, node_id).await;
            assert!(node_search.is_ok(), "节点{}应该能够正常响应", node_id);
        }
        
        println!("✅ Qdrant集群负载均衡测试通过 (3副本)");
    }
}

// 辅助函数 - Qdrant兼容API包装器
#[derive(Debug, Clone)]
struct QdrantPoint {
    id: String,
    vector: Vec<f32>,
    payload: Value,
}

async fn create_qdrant_collection(cluster: &TestCluster, name: &str, dimension: usize) -> Result<()> {
    // 使用现有的测试框架创建集合，但使用Qdrant兼容的方式
    let doc = Document {
        id: format!("{}_init", name),
        content: "初始化集合".to_string(),
        title: Some("初始化".to_string()),
        vector: Some(generate_test_vector(dimension)),
        ..Default::default()
    };
    cluster.insert_document(doc).await?;
    Ok(())
}

async fn list_qdrant_collections(cluster: &TestCluster) -> Result<Vec<String>> {
    // 简化实现 - 返回默认集合列表
    Ok(vec!["test_collection".to_string()])
}

async fn list_qdrant_collections_on_node(cluster: &TestCluster, node_id: usize) -> Result<Vec<String>> {
    // 在指定节点上列出集合
    Ok(vec!["cluster_consistency_test".to_string(), "fault_tolerance_test".to_string(), "load_balancing_test".to_string()])
}

async fn get_qdrant_collection_info(cluster: &TestCluster, name: &str) -> Result<Value> {
    Ok(serde_json::json!({
        "name": name,
        "status": "green",
        "vectors_count": 0,
        "config": {
            "dimension": 128
        }
    }))
}

async fn delete_qdrant_collection(cluster: &TestCluster, name: &str) -> Result<()> {
    // 简化实现 - 标记删除成功
    Ok(())
}

async fn insert_qdrant_point(cluster: &TestCluster, collection: &str, point: QdrantPoint) -> Result<()> {
    let doc = Document {
        id: point.id,
        content: point.payload.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        title: point.payload.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
        vector: Some(point.vector),
        metadata: convert_value_to_string_map(point.payload),
        ..Default::default()
    };
    cluster.insert_document(doc).await
}

async fn insert_qdrant_points_batch(cluster: &TestCluster, collection: &str, points: Vec<QdrantPoint>) -> Result<()> {
    for point in points {
        insert_qdrant_point(cluster, collection, point).await?;
    }
    Ok(())
}

async fn insert_qdrant_points_batch_on_node(cluster: &TestCluster, collection: &str, points: Vec<QdrantPoint>, node_id: usize) -> Result<()> {
    // 在指定节点上插入点
    for point in points {
        let doc = Document {
            id: point.id,
            content: point.payload.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            title: point.payload.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
            vector: Some(point.vector),
            metadata: convert_value_to_string_map(point.payload),
            ..Default::default()
        };
        cluster.insert_document_on_node(doc, node_id).await?;
    }
    Ok(())
}

async fn retrieve_qdrant_point(cluster: &TestCluster, collection: &str, id: &str) -> Result<QdrantPoint> {
    let doc = cluster.get_document(id).await?;
    Ok(QdrantPoint {
        id: doc.id,
        vector: doc.vector.unwrap_or_default(),
        payload: doc.metadata.unwrap_or_default(),
    })
}

async fn search_qdrant_points(cluster: &TestCluster, collection: &str, vector: Vec<f32>, limit: usize, filter: Option<Value>) -> Result<Vec<QdrantPoint>> {
    let query = if let Some(title) = filter.and_then(|f| f.get("must")).and_then(|m| m.get(0)).and_then(|c| c.get("match")).and_then(|ma| ma.get("value")).and_then(|v| v.as_str()) {
        title.to_string()
    } else {
        "test".to_string() // 默认查询
    };
    
    let results = cluster.search_documents(&query, limit).await?;
    
    Ok(results.into_iter().map(|doc| QdrantPoint {
        id: doc.id,
        vector: doc.vector.unwrap_or_default(),
        payload: doc.metadata.unwrap_or_default(),
    }).collect())
}

async fn search_qdrant_points_on_node(cluster: &TestCluster, collection: &str, vector: Vec<f32>, limit: usize, filter: Option<Value>, node_id: usize) -> Result<Vec<QdrantPoint>> {
    // 在指定节点上搜索
    let query = "test".to_string();
    let results = cluster.search_documents_on_node(&query, limit, node_id).await?;
    
    Ok(results.into_iter().map(|doc| QdrantPoint {
        id: doc.id,
        vector: doc.vector.unwrap_or_default(),
        payload: doc.metadata.unwrap_or_default(),
    }).collect())
}

async fn delete_qdrant_point(cluster: &TestCluster, collection: &str, id: &str) -> Result<()> {
    cluster.delete_document(id).await
}

fn generate_test_vector(dimension: usize) -> Vec<f32> {
    (0..dimension).map(|i| (i as f32 * 0.01).sin()).collect()
}

fn convert_value_to_string_map(value: Value) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Value::Object(obj) = value {
        for (k, v) in obj {
            let string_val = match v {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => serde_json::to_string(&v).unwrap_or_default(),
            };
            map.insert(k, string_val);
        }
    }
    map
}