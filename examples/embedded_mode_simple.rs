//! 内嵌模式基础示例
//! 
//! 这个示例展示了如何在应用程序中使用 Grape Vector Database 的内嵌模式基础功能。

use grape_vector_db::*;
use std::{collections::HashMap, time::Instant, path::PathBuf};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🍇 Grape Vector Database - 内嵌模式基础示例");

    // 运行各种示例场景
    basic_usage_example().await?;
    batch_operations_example().await?;
    performance_monitoring_example().await?;

    info!("✅ 所有示例运行完成");
    Ok(())
}

/// 基本使用示例
async fn basic_usage_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("📖 示例1: 基本使用");

    // 1. 创建向量数据库实例
    let config = VectorDbConfig::default();
    let db = VectorDatabase::new(PathBuf::from("./examples_data/basic"), config).await?;
    info!("数据库初始化完成");

    // 2. 添加一些文档
    let documents = vec![
        Document {
            id: "doc_1".to_string(),
            content: "Rust是一种系统编程语言，专注于安全、速度和并发性。".to_string(),
            title: Some("Rust编程语言".to_string()),
            language: Some("zh".to_string()),
            package_name: Some("rust-docs".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("tutorial".to_string()),
            vector: None,
            metadata: create_metadata(&[
                ("category", "编程语言"),
                ("difficulty", "中级"),
                ("tags", "系统编程,安全"),
            ]),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        Document {
            id: "doc_2".to_string(),
            content: "机器学习是人工智能的一个分支，让计算机能够从数据中学习。".to_string(),
            title: Some("机器学习基础".to_string()),
            language: Some("zh".to_string()),
            package_name: Some("ml-docs".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("tutorial".to_string()),
            vector: None,
            metadata: create_metadata(&[
                ("category", "人工智能"),
                ("difficulty", "初级"),
                ("tags", "机器学习,AI"),
            ]),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        Document {
            id: "doc_3".to_string(),
            content: "向量数据库是专门用于存储和搜索高维向量数据的数据库系统。".to_string(),
            title: Some("向量数据库介绍".to_string()),
            language: Some("zh".to_string()),
            package_name: Some("vector-db-docs".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("overview".to_string()),
            vector: None,
            metadata: create_metadata(&[
                ("category", "数据库"),
                ("difficulty", "中级"),
                ("tags", "向量,数据库,搜索"),
            ]),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    ];

    // 添加文档
    for doc in documents {
        let doc_id = db.add_document(doc).await?;
        info!("添加文档: {}", doc_id);
    }

    // 3. 搜索相似文档
    let search_queries = vec![
        "编程语言",
        "人工智能",
        "数据库系统",
    ];

    for query in search_queries {
        let results = db.text_search(query, 5).await?;
        info!("搜索 '{}' 的结果:", query);
        for (i, result) in results.iter().enumerate() {
            info!("  {}. {}: {:.4}", 
                  i + 1, 
                  result.document.title, 
                  result.score
            );
        }
    }

    // 4. 获取统计信息
    let stats = db.get_stats().await;
    info!("数据库统计:");
    info!("  文档数量: {}", stats.document_count);
    info!("  密集向量数量: {}", stats.dense_vector_count);

    info!("基本使用示例完成\n");
    Ok(())
}

/// 批量操作示例
async fn batch_operations_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("📊 示例2: 批量操作");

    let config = VectorDbConfig::default();
    let db = VectorDatabase::new(PathBuf::from("./examples_data/batch"), config).await?;

    // 准备批量文档数据
    let mut batch_documents = Vec::new();
    let categories = ["技术", "科学", "文学", "历史", "艺术"];
    
    for i in 0..25 {
        let category = categories[i % categories.len()];
        let doc = Document {
            id: format!("batch_doc_{}", i),
            content: format!("这是第{}个批量文档，属于{}类别。包含了相关的专业知识和信息。", i, category),
            title: Some(format!("批量文档 #{}", i)),
            language: Some("zh".to_string()),
            package_name: Some("batch-docs".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: create_metadata(&[
                ("category", category),
                ("batch_id", &(i / 10).to_string()),
                ("index", &i.to_string()),
            ]),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        batch_documents.push(doc);
    }

    // 执行批量添加
    let batch_start = Instant::now();
    let doc_ids = db.batch_add_documents(batch_documents).await?;
    let batch_time = batch_start.elapsed();
    
    info!("批量添加 {} 个文档完成，耗时: {:?}", doc_ids.len(), batch_time);

    // 测试不同类别的搜索
    for category in &categories {
        let results = db.text_search(&format!("{}相关内容", category), 3).await?;
        info!("{}类别搜索结果: {} 个", category, results.len());
    }

    // 获取最终统计
    let final_stats = db.get_stats().await;
    info!("批量操作后统计: {} 个文档", final_stats.document_count);

    info!("批量操作示例完成\n");
    Ok(())
}

/// 性能监控示例
async fn performance_monitoring_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("📈 示例3: 性能监控");

    let config = VectorDbConfig::default();
    let db = VectorDatabase::new(PathBuf::from("./examples_data/monitoring"), config).await?;

    // 添加一些测试数据
    let test_docs = (0..15).map(|i| Document {
        id: format!("perf_doc_{}", i),
        content: format!("性能测试文档 {}，用于监控数据库性能指标。", i),
        title: Some(format!("性能测试 #{}", i)),
        language: Some("zh".to_string()),
        package_name: Some("perf-test".to_string()),
        version: Some("1.0".to_string()),
        doc_type: Some("test".to_string()),
        vector: None,
        metadata: create_metadata(&[
            ("test_type", "performance"),
            ("index", &i.to_string()),
        ]),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }).collect::<Vec<_>>();

    let _doc_ids = db.batch_add_documents(test_docs).await?;

    // 执行多次搜索并监控性能
    let queries = vec![
        "性能测试",
        "数据库",
        "监控指标",
        "测试文档",
    ];

    for round in 1..=3 {
        info!("=== 性能测试轮次 {} ===", round);
        
        for query in &queries {
            let search_start = Instant::now();
            let results = db.text_search(query, 5).await?;
            let search_time = search_start.elapsed();
            
            info!("查询 '{}': {} 个结果, 耗时: {:?}", 
                  query, results.len(), search_time);
        }

        // 获取当前统计信息
        let stats = db.get_stats().await;
        info!("当前统计:");
        info!("  文档数量: {}", stats.document_count);
        info!("  密集向量数量: {}", stats.dense_vector_count);
        
        // 短暂休息
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    info!("性能监控示例完成\n");
    Ok(())
}

/// 创建元数据辅助函数
fn create_metadata(fields: &[(&str, &str)]) -> HashMap<String, String> {
    fields.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}