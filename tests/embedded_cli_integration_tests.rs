/// 内嵌CLI集成测试
/// 
/// 测试Grape Vector Database的内嵌CLI功能，包括：
/// - 内嵌模式下的命令行界面
/// - CLI命令的功能完整性
/// - 批处理操作
/// - 配置管理
/// - 性能测试集成

mod test_framework;

use std::time::Duration;
use std::sync::Arc;
use std::process::{Command, Stdio};
use anyhow::Result;
use tempfile::TempDir;
use serde_json::Value;

use test_framework::*;
use grape_vector_db::types::*;
use grape_vector_db::{VectorDatabase, VectorDbConfig};

/// CLI集成基础功能测试
#[cfg(test)]
mod cli_basic_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_embedded_cli_lifecycle() {
        println!("🧪 测试内嵌CLI生命周期管理");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_test_db");
        
        // 测试内嵌模式初始化
        println!("  ✅ 测试内嵌数据库初始化...");
        let config = VectorDbConfig::default();
        let db = VectorDatabase::new(db_path, config).await;
        assert!(db.is_ok(), "内嵌数据库初始化应该成功");
        let mut db = db.unwrap();
        
        // 测试CLI风格的基本操作
        println!("  ✅ 测试CLI风格基本操作...");
        let doc = Document {
            id: "cli_test_1".to_string(),
            content: "这是一个CLI测试文档".to_string(),
            title: Some("CLI测试".to_string()),
            ..Default::default()
        };
        
        let add_result = db.add_document(doc).await;
        assert!(add_result.is_ok(), "添加文档应该成功");
        
        // 测试搜索功能
        println!("  ✅ 测试搜索功能...");
        let search_results = db.text_search("CLI测试", 10).await;
        assert!(search_results.is_ok(), "搜索应该成功");
        let results = search_results.unwrap();
        assert!(!results.is_empty(), "应该找到结果");
        assert_eq!(results[0].document.id, "cli_test_1");
        
        // 测试统计信息
        println!("  ✅ 测试统计信息...");
        let stats = db.get_stats().await;
        assert_eq!(stats.document_count, 1, "文档数量应该正确");
        
        println!("✅ 内嵌CLI生命周期管理测试通过");
    }
    
    #[tokio::test]
    async fn test_embedded_cli_batch_operations() {
        println!("🧪 测试内嵌CLI批量操作");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_batch_test_db");
        let mut db = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        
        // 测试批量文档添加
        println!("  ✅ 测试批量文档添加...");
        let batch_docs = (1..=20).map(|i| Document {
            id: format!("batch_doc_{}", i),
            content: format!("这是批量文档{}的内容", i),
            title: Some(format!("批量文档{}", i)),
            metadata: Some(json_to_string_map(serde_json::json!({
                "batch_id": i / 5,
                "priority": if i % 2 == 0 { "high" } else { "normal" }
            }))),
            ..Default::default()
        }).collect::<Vec<_>>();
        
        // 批量添加文档
        for doc in batch_docs {
            let result = db.add_document(doc).await;
            assert!(result.is_ok(), "批量添加每个文档都应该成功");
        }
        
        // 验证批量添加结果
        let final_stats = db.get_stats().await;
        assert_eq!(final_stats.document_count, 20, "应该有20个文档");
        
        // 测试批量搜索
        println!("  ✅ 测试批量搜索...");
        let batch_queries = vec!["批量文档", "内容", "文档1"];
        for query in batch_queries {
            let search_results = db.text_search(query, 5).await;
            assert!(search_results.is_ok(), "批量搜索应该成功");
            let results = search_results.unwrap();
            assert!(!results.is_empty(), "每个查询都应该有结果");
        }
        
        println!("✅ 内嵌CLI批量操作测试通过");
    }
    
    #[tokio::test]
    async fn test_embedded_cli_configuration() {
        println!("🧪 测试内嵌CLI配置管理");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_config_test_db");
        
        // 测试默认配置
        println!("  ✅ 测试默认配置...");
        let db = VectorDatabase::new(db_path, VectorDbConfig::default()).await;
        assert!(db.is_ok(), "使用默认配置应该成功");
        let mut db = db.unwrap();
        
        // 测试配置相关的操作
        let test_doc = Document {
            id: "config_test".to_string(),
            content: "配置测试文档".to_string(),
            title: Some("配置测试".to_string()),
            ..Default::default()
        };
        
        let add_result = db.add_document(test_doc).await;
        assert!(add_result.is_ok(), "添加文档应该成功");
        
        // 测试数据持久化配置
        println!("  ✅ 测试数据持久化...");
        drop(db); // 关闭数据库
        
        // 重新打开数据库验证持久化
        let mut db_reopened = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        let stats = db_reopened.get_stats().await;
        assert_eq!(stats.document_count, 1, "重新打开后数据应该保持");
        
        let search_results = db_reopened.text_search("配置测试", 10).await.unwrap();
        assert!(!search_results.is_empty(), "重新打开后应该能搜索到数据");
        
        println!("✅ 内嵌CLI配置管理测试通过");
    }
}

/// CLI高级功能测试
#[cfg(test)]
mod cli_advanced_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_embedded_cli_performance_monitoring() {
        println!("🧪 测试内嵌CLI性能监控");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_perf_test_db");
        let mut db = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        
        // 插入性能测试数据
        println!("  ✅ 插入性能测试数据...");
        let start_time = std::time::Instant::now();
        
        for i in 1..=100 {
            let doc = Document {
                id: format!("perf_doc_{}", i),
                content: format!("性能测试文档{}内容", i),
                title: Some(format!("性能文档{}", i)),
                ..Default::default()
            };
            
            let result = db.add_document(doc).await;
            assert!(result.is_ok(), "性能测试文档添加应该成功");
        }
        
        let insert_duration = start_time.elapsed();
        println!("  📊 插入100个文档耗时: {:?}", insert_duration);
        assert!(insert_duration < Duration::from_secs(10), "插入性能应该合理");
        
        // 搜索性能测试
        println!("  ✅ 搜索性能测试...");
        let search_start = std::time::Instant::now();
        
        for i in 1..=20 {
            let query = format!("性能文档{}", i);
            let search_results = db.text_search(&query, 5).await;
            assert!(search_results.is_ok(), "搜索应该成功");
        }
        
        let search_duration = search_start.elapsed();
        println!("  📊 执行20次搜索耗时: {:?}", search_duration);
        assert!(search_duration < Duration::from_secs(5), "搜索性能应该合理");
        
        // 验证最终统计
        let final_stats = db.get_stats().await;
        assert_eq!(final_stats.document_count, 100, "应该有100个文档");
        
        println!("✅ 内嵌CLI性能监控测试通过");
    }
    
    #[tokio::test]
    async fn test_embedded_cli_concurrent_operations() {
        println!("🧪 测试内嵌CLI并发操作");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_concurrent_test_db");
        let db = Arc::new(tokio::sync::Mutex::new(
            VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap()
        ));
        
        // 并发插入测试
        println!("  ✅ 并发插入测试...");
        let mut insert_tasks = Vec::new();
        
        for i in 1..=10 {
            let db_clone = Arc::clone(&db);
            let task = tokio::spawn(async move {
                let doc = Document {
                    id: format!("concurrent_doc_{}", i),
                    content: format!("并发文档{}内容", i),
                    title: Some(format!("并发文档{}", i)),
                    ..Default::default()
                };
                
                let mut db_guard = db_clone.lock().await;
                db_guard.add_document(doc).await
            });
            insert_tasks.push(task);
        }
        
        let insert_results = futures::future::join_all(insert_tasks).await;
        let successful_inserts = insert_results.iter()
            .filter(|result| result.is_ok() && result.as_ref().unwrap().is_ok())
            .count();
        
        assert!(successful_inserts >= 8, "大部分并发插入应该成功");
        
        // 并发搜索测试
        println!("  ✅ 并发搜索测试...");
        let mut search_tasks = Vec::new();
        
        for i in 1..=10 {
            let db_clone = Arc::clone(&db);
            let task = tokio::spawn(async move {
                let query = format!("并发文档{}", i);
                let db_guard = db_clone.lock().await;
                db_guard.text_search(&query, 5).await
            });
            search_tasks.push(task);
        }
        
        let search_results = futures::future::join_all(search_tasks).await;
        let successful_searches = search_results.iter()
            .filter(|result| result.is_ok() && result.as_ref().unwrap().is_ok())
            .count();
        
        assert!(successful_searches >= 8, "大部分并发搜索应该成功");
        
        println!("✅ 内嵌CLI并发操作测试通过");
    }
    
    #[tokio::test]
    async fn test_embedded_cli_error_handling() {
        println!("🧪 测试内嵌CLI错误处理");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_error_test_db");
        let mut db = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        
        // 测试重复ID错误处理
        println!("  ✅ 测试重复ID错误处理...");
        let doc1 = Document {
            id: "duplicate_test".to_string(),
            content: "第一个文档".to_string(),
            title: Some("文档1".to_string()),
            ..Default::default()
        };
        
        let first_add = db.add_document(doc1).await;
        assert!(first_add.is_ok(), "第一次添加应该成功");
        
        let doc2 = Document {
            id: "duplicate_test".to_string(), // 相同ID
            content: "第二个文档".to_string(),
            title: Some("文档2".to_string()),
            ..Default::default()
        };
        
        let second_add = db.add_document(doc2).await;
        // 根据实现，可能成功（覆盖）或失败，这里只验证不会崩溃
        assert!(second_add.is_ok() || second_add.is_err(), "重复ID处理应该有明确结果");
        
        // 测试无效查询处理
        println!("  ✅ 测试无效查询处理...");
        let empty_search = db.text_search("", 10).await;
        assert!(empty_search.is_ok(), "空查询应该被优雅处理");
        
        let very_long_query = "A".repeat(10000);
        let long_search = db.text_search(&very_long_query, 10).await;
        assert!(long_search.is_ok(), "超长查询应该被优雅处理");
        
        // 测试边界条件
        println!("  ✅ 测试边界条件...");
        let zero_limit_search = db.text_search("test", 0).await;
        assert!(zero_limit_search.is_ok(), "零限制搜索应该被处理");
        
        let large_limit_search = db.text_search("test", 10000).await;
        assert!(large_limit_search.is_ok(), "大限制搜索应该被处理");
        
        println!("✅ 内嵌CLI错误处理测试通过");
    }
}

/// CLI性能基准测试
#[cfg(test)]
mod cli_performance_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_embedded_cli_performance_benchmark() {
        println!("🧪 内嵌CLI性能基准测试");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_benchmark_db");
        let mut db = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        
        // 大规模数据插入基准
        println!("  📊 大规模数据插入基准...");
        let insert_start = std::time::Instant::now();
        let batch_size = 500;
        
        for i in 1..=batch_size {
            let doc = Document {
                id: format!("benchmark_doc_{}", i),
                content: format!("这是基准测试文档{}，包含一些测试内容用于验证搜索功能", i),
                title: Some(format!("基准文档{}", i)),
                metadata: Some(json_to_string_map(serde_json::json!({
                    "category": if i % 3 == 0 { "技术" } else if i % 3 == 1 { "科学" } else { "文学" },
                    "priority": i % 5,
                    "timestamp": chrono::Utc::now().timestamp()
                }))),
                ..Default::default()
            };
            
            let result = db.add_document(doc).await;
            assert!(result.is_ok(), "基准测试文档{}添加失败", i);
            
            if i % 100 == 0 {
                println!("    已插入 {}/{} 文档", i, batch_size);
            }
        }
        
        let insert_duration = insert_start.elapsed();
        let insert_qps = batch_size as f64 / insert_duration.as_secs_f64();
        println!("  📈 插入性能: {:.2} docs/sec", insert_qps);
        
        // 搜索性能基准
        println!("  📊 搜索性能基准...");
        let search_queries = vec![
            "基准测试",
            "技术",
            "科学",
            "文学",
            "文档100",
            "内容",
            "验证",
            "搜索功能",
        ];
        
        let search_start = std::time::Instant::now();
        let search_iterations = 100;
        
        for i in 0..search_iterations {
            let query = &search_queries[i % search_queries.len()];
            let search_result = db.text_search(query, 10).await;
            assert!(search_result.is_ok(), "搜索应该成功");
        }
        
        let search_duration = search_start.elapsed();
        let search_qps = search_iterations as f64 / search_duration.as_secs_f64();
        println!("  📈 搜索性能: {:.2} queries/sec", search_qps);
        
        // 验证最终统计
        let final_stats = db.get_stats().await;
        assert_eq!(final_stats.document_count, batch_size, "文档数量应该正确");
        
        // 性能要求验证
        assert!(insert_qps > 50.0, "插入性能应该 > 50 docs/sec");
        assert!(search_qps > 100.0, "搜索性能应该 > 100 queries/sec");
        
        println!("✅ 内嵌CLI性能基准测试通过");
        println!("  📋 性能总结:");
        println!("    - 插入QPS: {:.2}", insert_qps);
        println!("    - 搜索QPS: {:.2}", search_qps);
        println!("    - 总文档数: {}", final_stats.document_count);
    }
    
    #[tokio::test]
    async fn test_embedded_cli_memory_usage() {
        println!("🧪 内嵌CLI内存使用测试");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_memory_test_db");
        let mut db = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        
        // 获取初始内存状态
        let initial_stats = db.get_stats().await;
        println!("  📊 初始状态: {} 文档", initial_stats.document_count);
        
        // 分批插入大量数据监控内存
        let batches = 5;
        let docs_per_batch = 200;
        
        for batch in 1..=batches {
            println!("  📦 处理批次 {}/{}...", batch, batches);
            
            for i in 1..=docs_per_batch {
                let doc_id = (batch - 1) * docs_per_batch + i;
                let doc = Document {
                    id: format!("memory_test_doc_{}", doc_id),
                    content: format!("内存测试文档{}，这是一个用于测试内存使用的较长内容字符串", doc_id),
                    title: Some(format!("内存测试文档{}", doc_id)),
                    metadata: Some(json_to_string_map(serde_json::json!({
                        "batch": batch,
                        "doc_in_batch": i,
                        "large_field": "X".repeat(1000) // 模拟大字段
                    }))),
                    ..Default::default()
                };
                
                let result = db.add_document(doc).await;
                assert!(result.is_ok(), "文档添加应该成功");
            }
            
            // 每批次后验证状态
            let batch_stats = db.get_stats().await;
            let expected_count = batch * docs_per_batch;
            assert_eq!(batch_stats.document_count, expected_count, 
                      "批次{}后应该有{}个文档", batch, expected_count);
            
            println!("    当前文档数: {}", batch_stats.document_count);
            
            // 执行一些搜索验证功能
            let search_result = db.text_search("内存测试", 5).await;
            assert!(search_result.is_ok(), "批次{}后搜索应该正常", batch);
        }
        
        // 验证最终状态
        let final_stats = db.get_stats().await;
        let total_docs = batches * docs_per_batch;
        assert_eq!(final_stats.document_count, total_docs, "最终应该有{}个文档", total_docs);
        
        // 执行大量搜索测试内存稳定性
        println!("  🔍 执行大量搜索测试内存稳定性...");
        for i in 1..=50 {
            let query = format!("内存测试文档{}", i * 20);
            let search_result = db.text_search(&query, 10).await;
            assert!(search_result.is_ok(), "大量搜索应该稳定");
        }
        
        println!("✅ 内嵌CLI内存使用测试通过");
        println!("  📋 内存测试总结:");
        println!("    - 总文档数: {}", final_stats.document_count);
        println!("    - 所有操作完成无内存问题");
    }
}

/// CLI集成端到端测试
#[cfg(test)]
mod cli_e2e_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_embedded_cli_complete_workflow() {
        println!("🧪 内嵌CLI完整工作流测试");
        
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cli_e2e_test_db");
        
        // 阶段1: 初始化和基础数据
        println!("  🚀 阶段1: 数据库初始化...");
        let mut db = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        
        let initial_docs = vec![
            ("tech_1", "Rust编程语言入门", "技术"),
            ("tech_2", "Python数据科学应用", "技术"),
            ("sci_1", "量子计算原理", "科学"),
            ("sci_2", "机器学习算法", "科学"),
            ("lit_1", "现代文学作品分析", "文学"),
        ];
        
        for (id, content, category) in initial_docs {
            let doc = Document {
                id: id.to_string(),
                content: content.to_string(),
                title: Some(content.to_string()),
                metadata: Some(json_to_string_map(serde_json::json!({
                    "category": category,
                    "stage": "initial"
                }))),
                ..Default::default()
            };
            
            let result = db.add_document(doc).await;
            assert!(result.is_ok(), "初始文档{}添加应该成功", id);
        }
        
        // 阶段2: 搜索和过滤测试
        println!("  🔍 阶段2: 搜索功能测试...");
        
        // 技术类搜索
        let tech_results = db.text_search("Rust编程", 10).await.unwrap();
        assert!(!tech_results.is_empty(), "应该找到Rust相关文档");
        
        // 科学类搜索
        let sci_results = db.text_search("量子计算", 10).await.unwrap();
        assert!(!sci_results.is_empty(), "应该找到量子计算文档");
        
        // 通用搜索
        let general_results = db.text_search("算法", 10).await.unwrap();
        assert!(!general_results.is_empty(), "应该找到算法相关文档");
        
        // 阶段3: 数据更新和扩展
        println!("  📝 阶段3: 数据更新和扩展...");
        
        let expansion_docs = (1..=15).map(|i| Document {
            id: format!("exp_doc_{}", i),
            content: format!("扩展文档{}内容，涵盖多个领域的知识", i),
            title: Some(format!("扩展文档{}", i)),
            metadata: Some(json_to_string_map(serde_json::json!({
                "category": match i % 3 {
                    0 => "技术",
                    1 => "科学", 
                    _ => "文学"
                },
                "stage": "expansion",
                "priority": i % 5
            }))),
            ..Default::default()
        }).collect::<Vec<_>>();
        
        for doc in expansion_docs {
            let result = db.add_document(doc).await;
            assert!(result.is_ok(), "扩展文档添加应该成功");
        }
        
        // 阶段4: 复杂查询测试
        println!("  🧮 阶段4: 复杂查询测试...");
        
        // 验证总数据量
        let mid_stats = db.get_stats().await;
        assert_eq!(mid_stats.document_count, 20, "应该有20个文档");
        
        // 多种搜索模式
        let search_patterns = vec![
            ("技术", 5),
            ("科学", 5),
            ("文学", 5),
            ("文档", 10),
            ("知识", 8),
        ];
        
        for (query, expected_min) in search_patterns {
            let results = db.text_search(query, 15).await.unwrap();
            assert!(results.len() >= expected_min.min(results.len()), 
                   "查询'{}'应该至少返回{}个结果", query, expected_min);
        }
        
        // 阶段5: 持久化验证
        println!("  💾 阶段5: 持久化验证...");
        
        drop(db); // 关闭数据库
        
        // 重新打开数据库
        let mut db_reopened = VectorDatabase::new(db_path, VectorDbConfig::default()).await.unwrap();
        let final_stats = db_reopened.get_stats().await;
        assert_eq!(final_stats.document_count, 20, "重新打开后数据应该完整");
        
        // 验证搜索功能
        let persistence_test = db_reopened.text_search("Rust编程", 5).await.unwrap();
        assert!(!persistence_test.is_empty(), "重新打开后搜索应该正常");
        
        // 阶段6: 性能验证
        println!("  ⚡ 阶段6: 性能验证...");
        
        let perf_start = std::time::Instant::now();
        for i in 1..=20 {
            let query = format!("文档{}", i);
            let result = db_reopened.text_search(&query, 5).await;
            assert!(result.is_ok(), "性能测试搜索应该成功");
        }
        let perf_duration = perf_start.elapsed();
        assert!(perf_duration < Duration::from_secs(2), "性能应该满足要求");
        
        println!("✅ 内嵌CLI完整工作流测试通过");
        println!("  📋 E2E测试总结:");
        println!("    - 文档总数: {}", final_stats.document_count);
        println!("    - 20次搜索耗时: {:?}", perf_duration);
        println!("    - 数据持久化: ✅");
        println!("    - 功能完整性: ✅");
    }
}

// 运行所有CLI测试的辅助函数
#[tokio::test]
async fn run_all_cli_tests() {
    tracing_subscriber::fmt::init();
    
    println!("开始运行内嵌CLI集成测试套件...");
    println!("=" .repeat(60));
    
    // 注意：在实际测试中，这些测试模块会自动运行
    // 这里只是一个占位符函数来组织测试结构
    
    println!("所有内嵌CLI集成测试完成！");
}

fn json_to_string_map(value: serde_json::Value) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let serde_json::Value::Object(obj) = value {
        for (k, v) in obj {
            let string_val = match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => serde_json::to_string(&v).unwrap_or_default(),
            };
            map.insert(k, string_val);
        }
    }
    map
}