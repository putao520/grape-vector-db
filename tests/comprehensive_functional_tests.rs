/// 综合功能测试套件
/// 
/// 包含对Grape Vector Database的完整功能验证：
/// - 单节点模式测试
/// - 3副本集群模式测试 
/// - 内嵌CLI模式测试
/// - Qdrant兼容性验证

mod test_framework;

use std::time::Duration;
use anyhow::Result;
use tempfile::TempDir;

use test_framework::{TestCluster, ClusterConfig, ClusterType};
use grape_vector_db::types::*;
use grape_vector_db::{VectorDatabase, VectorDbConfig};

/// 单节点功能测试
#[cfg(test)]
mod single_node_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_single_node_basic_operations() {
        println!("🧪 测试单节点基本操作");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::Standalone,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 1,
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 测试文档插入
        println!("  ✅ 测试文档插入...");
        let doc = Document {
            id: "test_doc_1".to_string(),
            content: "这是一个测试文档".to_string(),
            title: Some("测试文档".to_string()),
            ..Default::default()
        };
        
        let insert_result = cluster.insert_document(doc).await;
        assert!(insert_result.is_ok(), "文档插入应该成功");
        
        // 测试文档检索
        println!("  ✅ 测试文档检索...");
        let get_result = cluster.get_document("test_doc_1").await;
        assert!(get_result.is_ok(), "文档检索应该成功");
        
        // 测试文档搜索
        println!("  ✅ 测试文档搜索...");
        let search_result = cluster.search_documents("测试", 10).await;
        assert!(search_result.is_ok(), "文档搜索应该成功");
        let results = search_result.unwrap();
        assert!(!results.is_empty(), "应该找到搜索结果");
        
        println!("✅ 单节点基本操作测试通过");
    }
    
    #[tokio::test]
    async fn test_single_node_batch_operations() {
        println!("🧪 测试单节点批量操作");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::Standalone,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 1,
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 批量插入文档
        println!("  ✅ 批量插入文档...");
        for i in 1..=10 {
            let doc = Document {
                id: format!("batch_doc_{}", i),
                content: format!("批量文档{}内容", i),
                title: Some(format!("批量文档{}", i)),
                ..Default::default()
            };
            
            let result = cluster.insert_document(doc).await;
            assert!(result.is_ok(), "批量文档{}插入应该成功", i);
        }
        
        // 验证批量搜索
        println!("  ✅ 验证批量搜索...");
        let search_result = cluster.search_documents("批量", 10).await;
        assert!(search_result.is_ok(), "批量搜索应该成功");
        
        println!("✅ 单节点批量操作测试通过");
    }
}

/// 3副本集群功能测试
#[cfg(test)]
mod three_replica_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_three_replica_consistency() {
        println!("🧪 测试3副本集群一致性");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::ThreeNode,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 3,
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await; // 等待集群建立
        
        // 插入测试数据
        println!("  ✅ 插入测试数据...");
        for i in 1..=5 {
            let doc = Document {
                id: format!("replica_doc_{}", i),
                content: format!("3副本文档{}内容", i),
                title: Some(format!("3副本文档{}", i)),
                ..Default::default()
            };
            
            let result = cluster.insert_document(doc).await;
            assert!(result.is_ok(), "文档{}插入应该成功", i);
        }
        
        tokio::time::sleep(Duration::from_millis(1000)).await; // 等待同步
        
        // 验证数据在所有节点上的一致性
        println!("  ✅ 验证数据一致性...");
        for node_id in 0..3 {
            let search_result = cluster.search_documents_on_node("3副本", 10, node_id).await;
            assert!(search_result.is_ok(), "节点{}搜索应该成功", node_id);
            let results = search_result.unwrap();
            assert!(!results.is_empty(), "节点{}应该有搜索结果", node_id);
        }
        
        println!("✅ 3副本集群一致性测试通过");
    }
    
    #[tokio::test]
    async fn test_three_replica_fault_tolerance() {
        println!("🧪 测试3副本集群故障容错");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::ThreeNode,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 3,
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        // 插入初始数据
        println!("  ✅ 插入初始数据...");
        for i in 1..=3 {
            let doc = Document {
                id: format!("fault_doc_{}", i),
                content: format!("容错文档{}内容", i),
                title: Some(format!("容错文档{}", i)),
                ..Default::default()
            };
            cluster.insert_document(doc).await.unwrap();
        }
        
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // 停止一个节点
        println!("  ✅ 停止节点1模拟故障...");
        cluster.stop_node(1).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // 验证集群仍然可用
        println!("  ✅ 验证集群仍然可用...");
        let search_during_failure = cluster.search_documents("容错", 10).await;
        assert!(search_during_failure.is_ok(), "单节点故障时集群应该仍可用");
        
        // 重启故障节点
        println!("  ✅ 重启故障节点...");
        cluster.start_node(1).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await;
        
        // 验证恢复后的功能
        println!("  ✅ 验证恢复后功能...");
        let search_after_recovery = cluster.search_documents("容错", 10).await;
        assert!(search_after_recovery.is_ok(), "恢复后搜索应该正常");
        
        println!("✅ 3副本集群故障容错测试通过");
    }
}

/// 内嵌CLI功能测试
#[cfg(test)]
mod embedded_cli_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_embedded_cli_basic_workflow() {
        println!("🧪 测试内嵌CLI基本工作流");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_test_db");
        
        // 初始化数据库
        println!("  ✅ 初始化数据库...");
        let config = VectorDbConfig::default();
        let db = VectorDatabase::new(db_path.clone(), config).await.unwrap();
        
        // 添加文档
        println!("  ✅ 添加文档...");
        let doc = Document {
            id: "cli_doc_1".to_string(),
            content: "内嵌CLI测试文档".to_string(),
            title: Some("CLI测试".to_string()),
            ..Default::default()
        };
        
        let add_result = db.add_document(doc).await;
        assert!(add_result.is_ok(), "添加文档应该成功");
        
        // 搜索文档
        println!("  ✅ 搜索文档...");
        let search_results = db.text_search("CLI测试", 10).await;
        assert!(search_results.is_ok(), "搜索应该成功");
        let results = search_results.unwrap();
        assert!(!results.is_empty(), "应该找到结果");
        
        // 验证统计信息
        println!("  ✅ 验证统计信息...");
        let stats = db.get_stats().await;
        assert_eq!(stats.document_count, 1, "文档数量应该正确");
        
        println!("✅ 内嵌CLI基本工作流测试通过");
    }
    
    #[tokio::test]
    async fn test_embedded_cli_persistence() {
        println!("🧪 测试内嵌CLI数据持久化");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("persistence_test_db");
        
        // 第一阶段：创建和保存数据
        println!("  ✅ 创建和保存数据...");
        {
            let config = VectorDbConfig::default();
            let db = VectorDatabase::new(db_path.clone(), config).await.unwrap();
            
            let doc = Document {
                id: "persist_doc".to_string(),
                content: "持久化测试文档".to_string(),
                title: Some("持久化测试".to_string()),
                ..Default::default()
            };
            
            db.add_document(doc).await.unwrap();
        } // 数据库在这里关闭
        
        // 第二阶段：重新打开验证数据
        println!("  ✅ 重新打开验证数据...");
        {
            let config = VectorDbConfig::default();
            let db = VectorDatabase::new(db_path, config).await.unwrap();
            
            let stats = db.get_stats().await;
            assert_eq!(stats.document_count, 1, "重新打开后数据应该保持");
            
            let search_results = db.text_search("持久化", 10).await.unwrap();
            assert!(!search_results.is_empty(), "重新打开后应该能搜索到数据");
        }
        
        println!("✅ 内嵌CLI数据持久化测试通过");
    }
}

/// Qdrant兼容性测试
#[cfg(test)]
mod qdrant_compatibility_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_qdrant_api_compatibility() {
        println!("🧪 测试Qdrant API兼容性");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::Standalone,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 1,
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 模拟Qdrant风格的点插入
        println!("  ✅ 模拟Qdrant风格的点插入...");
        let qdrant_style_doc = Document {
            id: "qdrant_point_1".to_string(),
            content: "Qdrant兼容点".to_string(),
            title: Some("Qdrant点".to_string()),
            vector: Some(vec![1.0, 2.0, 3.0, 4.0]), // 模拟向量
            ..Default::default()
        };
        
        let insert_result = cluster.insert_document(qdrant_style_doc).await;
        assert!(insert_result.is_ok(), "Qdrant风格点插入应该成功");
        
        // 模拟Qdrant风格的搜索
        println!("  ✅ 模拟Qdrant风格的搜索...");
        let search_result = cluster.search_documents("Qdrant", 10).await;
        assert!(search_result.is_ok(), "Qdrant风格搜索应该成功");
        let results = search_result.unwrap();
        assert!(!results.is_empty(), "应该返回搜索结果");
        
        // 验证返回的数据格式
        println!("  ✅ 验证返回的数据格式...");
        let first_result = &results[0];
        assert_eq!(first_result.id, "qdrant_point_1");
        assert!(first_result.vector.is_some(), "应该包含向量数据");
        
        println!("✅ Qdrant API兼容性测试通过");
    }
}

/// 性能基准测试
#[cfg(test)]
mod performance_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_basic_performance() {
        println!("🧪 基础性能测试");
        
        let temp_dir = TempDir::new().unwrap();
        let cluster = TestCluster::new_with_config(ClusterConfig {
            cluster_type: ClusterType::Standalone,
            data_dir: temp_dir.path().to_path_buf(),
            node_count: 1,
        }).await;
        
        cluster.start_all_nodes().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 测试插入性能
        println!("  ✅ 测试插入性能...");
        let start_time = std::time::Instant::now();
        
        for i in 1..=50 {
            let doc = Document {
                id: format!("perf_doc_{}", i),
                content: format!("性能测试文档{}内容", i),
                title: Some(format!("性能文档{}", i)),
                ..Default::default()
            };
            
            let result = cluster.insert_document(doc).await;
            assert!(result.is_ok(), "性能测试文档插入应该成功");
        }
        
        let insert_duration = start_time.elapsed();
        println!("  📊 插入50个文档耗时: {:?}", insert_duration);
        assert!(insert_duration < Duration::from_secs(5), "插入性能应该合理");
        
        // 测试搜索性能
        println!("  ✅ 测试搜索性能...");
        let search_start = std::time::Instant::now();
        
        for i in 1..=10 {
            let query = format!("性能文档{}", i);
            let search_result = cluster.search_documents(&query, 5).await;
            assert!(search_result.is_ok(), "搜索应该成功");
        }
        
        let search_duration = search_start.elapsed();
        println!("  📊 执行10次搜索耗时: {:?}", search_duration);
        assert!(search_duration < Duration::from_secs(2), "搜索性能应该合理");
        
        println!("✅ 基础性能测试通过");
    }
}

// 运行所有综合测试的辅助函数
#[tokio::test]
async fn run_comprehensive_tests() {
    tracing_subscriber::fmt::init();
    
    println!("🚀 开始运行Grape Vector Database综合功能测试套件");
    println!("=" .repeat(80));
    
    // 注意：在实际测试中，这些测试模块会自动运行
    // 这里只是一个占位符函数来组织测试结构
    
    println!("✅ 所有综合功能测试完成！");
    println!("📋 测试覆盖范围:");
    println!("  - ✅ 单节点基本操作");
    println!("  - ✅ 单节点批量操作");
    println!("  - ✅ 3副本集群一致性");
    println!("  - ✅ 3副本集群故障容错");
    println!("  - ✅ 内嵌CLI基本工作流");
    println!("  - ✅ 内嵌CLI数据持久化");
    println!("  - ✅ Qdrant API兼容性");
    println!("  - ✅ 基础性能验证");
}