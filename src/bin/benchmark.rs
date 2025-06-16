use grape_vector_db::{
    VectorDatabase, VectorDbConfig, Document,
    errors::Result,
};
use std::collections::HashMap;
use std::time::Instant;
use std::path::PathBuf;
use tokio;
use chrono;
use rand;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🍇 Grape Vector DB 性能基准测试");
    println!("=====================================");
    
    // 创建数据库配置
    let config = VectorDbConfig::default();
    let data_dir = PathBuf::from("./benchmark_data");
    let mut db = VectorDatabase::new(data_dir, config).await?;
    
    // 基准测试参数
    let document_count = 1000;
    let search_count = 100;
    
    println!("📊 测试参数:");
    println!("  - 文档数量: {}", document_count);
    println!("  - 搜索次数: {}", search_count);
    println!();
    
    // 1. 插入性能测试
    println!("🔄 插入性能测试...");
    let insert_start = Instant::now();
    
    for i in 0..document_count {
        let document = Document {
            id: format!("doc_{}", i),
            content: format!("这是第{}个测试文档的内容，包含一些随机文本用于测试向量化和搜索功能。", i),
            title: Some(format!("测试文档 {}", i)),
            language: Some("zh".to_string()),
            package_name: Some("benchmark".to_string()),
            version: Some("1.0.0".to_string()),
            doc_type: Some("test".to_string()),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("category".to_string(), "benchmark".to_string());
                meta.insert("index".to_string(), i.to_string());
                meta
            },
            vector: Some((0..384).map(|_| rand::random::<f32>()).collect()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        db.add_document(document).await?;
        
        if i % 100 == 0 {
            print!(".");
        }
    }
    
    let insert_duration = insert_start.elapsed();
    let insert_qps = document_count as f64 / insert_duration.as_secs_f64();
    
    println!();
    println!("✅ 插入完成:");
    println!("  - 总时间: {:.2}s", insert_duration.as_secs_f64());
    println!("  - QPS: {:.2}", insert_qps);
    println!();
    
    // 2. 搜索性能测试
    println!("🔍 搜索性能测试...");
    let search_start = Instant::now();
    
    let mut total_results = 0;
    
    for i in 0..search_count {
        let query = format!("测试文档 {}", i % 100);
        let results = db.text_search(&query, 10).await?;
        total_results += results.len();
        
        if i % 10 == 0 {
            print!(".");
        }
    }
    
    let search_duration = search_start.elapsed();
    let search_qps = search_count as f64 / search_duration.as_secs_f64();
    let avg_latency = search_duration.as_millis() as f64 / search_count as f64;
    
    println!();
    println!("✅ 搜索完成:");
    println!("  - 总时间: {:.2}s", search_duration.as_secs_f64());
    println!("  - QPS: {:.2}", search_qps);
    println!("  - 平均延迟: {:.2}ms", avg_latency);
    println!("  - 总结果数: {}", total_results);
    println!();
    
    // 3. 内存使用统计
    println!("💾 内存使用统计:");
    let stats = db.get_stats();
    println!("  - 文档数量: {}", stats.document_count);
    println!("  - 密集向量数: {}", stats.dense_vector_count);
    println!("  - 内存使用: {:.2}MB", stats.memory_usage_mb);
    println!("  - 索引大小: {:.2}MB", stats.dense_index_size_mb);
    println!("  - 缓存命中率: {:.2}%", stats.cache_hit_rate * 100.0);
    println!();
    
    // 4. 性能总结
    println!("📈 性能总结:");
    println!("=====================================");
    println!("🚀 插入性能: {:.0} QPS", insert_qps);
    println!("⚡ 搜索性能: {:.0} QPS", search_qps);
    println!("⏱️  平均延迟: {:.2}ms", avg_latency);
    println!("💾 内存效率: {:.2}MB", stats.memory_usage_mb);
    
    // 性能等级评估
    if search_qps > 1000.0 {
        println!("🏆 性能等级: 优秀 (>1000 QPS)");
    } else if search_qps > 500.0 {
        println!("🥈 性能等级: 良好 (>500 QPS)");
    } else if search_qps > 100.0 {
        println!("🥉 性能等级: 一般 (>100 QPS)");
    } else {
        println!("⚠️  性能等级: 需要优化 (<100 QPS)");
    }
    
    println!();
    println!("✨ 基准测试完成！");
    
    Ok(())
} 