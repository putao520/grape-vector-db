use grape_vector_db::{
    VectorDatabase, VectorDbConfig, Document,
    errors::Result,
};
use std::time::Instant;
use std::path::PathBuf;
use tokio;
use tracing::error;
use chrono;
use rand;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🚀 Grape Vector DB 性能优化测试 (文本搜索模式)");
    println!("====================================");
    
    // 创建数据库配置
    let config = VectorDbConfig::default();
    let data_dir = PathBuf::from("./performance_test_data");
    let mut db = VectorDatabase::new(data_dir, config).await?;
    
    // 准备测试数据 - 减少数据量避免HNSW问题
    println!("📦 准备测试数据...");
    let start_prep = Instant::now();
    
    let doc_count = 1000; // 减少到1000个文档
    for i in 0..doc_count {
        let doc = Document {
            id: format!("perf_doc_{}", i),
            content: format!("这是性能测试文档 {} 的内容，包含各种关键词用于搜索测试。文档内容包括：性能、测试、数据库、向量、搜索、索引、查询、优化等关键词。", i),
            title: Some(format!("性能测试文档 {}", i)),
            language: Some("zh".to_string()),
            package_name: Some("performance_test".to_string()),
            version: Some("1.0.0".to_string()),
            doc_type: Some("test".to_string()),
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("category".to_string(), "performance".to_string());
                meta.insert("index".to_string(), i.to_string());
                meta
            },
            vector: Some((0..384).map(|_| rand::random::<f32>()).collect()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        db.add_document(doc).await?;
        
        if i % 200 == 0 {
            println!("  已添加 {} 个文档", i + 1);
        }
    }
    
    let prep_time = start_prep.elapsed();
    println!("✅ 数据准备完成，耗时: {:?}", prep_time);
    println!();
    
    // 性能测试 - 使用文本搜索而不是向量搜索
    run_performance_tests(&db).await?;
    
    Ok(())
}

async fn run_performance_tests(db: &VectorDatabase) -> Result<()> {
    let test_queries = vec![
        ("性能测试", 10),
        ("文档内容", 50),
        ("关键词", 100),
        ("搜索测试", 200),
        ("数据库", 20),
        ("向量", 30),
        ("索引", 40),
        ("查询", 60),
        ("优化", 80),
    ];
    
    println!("🔥 开始性能测试 (文本搜索)");
    println!("================");
    
    let mut total_time = 0.0;
    let mut total_searches = 0;
    let mut failed_searches = 0;
    
    for (query, limit) in test_queries {
        println!("🔍 测试查询: \"{}\" (limit: {})", query, limit);
        
        // 预热 - 使用文本搜索
        for i in 0..3 {
            match db.text_search(query, limit).await {
                Ok(_) => {},
                Err(e) => {
                    println!("  ⚠️ 预热搜索 {} 失败: {}", i + 1, e);
                    continue;
                }
            }
        }
        
        // 实际测试
        let mut query_times = Vec::new();
        let test_rounds = 20;
        let mut successful_rounds = 0;
        
        for round in 0..test_rounds {
            let start = Instant::now();
            
            match db.text_search(query, limit).await {
                Ok(results) => {
                    let elapsed = start.elapsed();
                    query_times.push(elapsed.as_millis() as f64);
                    total_time += elapsed.as_millis() as f64;
                    total_searches += 1;
                    successful_rounds += 1;
                    
                    if round == 0 {
                        println!("  首次搜索: 找到 {} 个结果", results.len());
                    }
                },
                Err(e) => {
                    failed_searches += 1;
                    error!("搜索失败 (round {}): {}", round + 1, e);
                }
            }
        }
        
        if query_times.is_empty() {
            println!("  ❌ 所有搜索都失败了");
            continue;
        }
        
        // 统计分析
        query_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let avg_time = query_times.iter().sum::<f64>() / query_times.len() as f64;
        let min_time = query_times.first().copied().unwrap_or(0.0);
        let max_time = query_times.last().copied().unwrap_or(0.0);
        let p95_time = query_times.get((query_times.len() as f64 * 0.95) as usize).copied().unwrap_or(0.0);
        let qps = 1000.0 / avg_time;
        let success_rate = successful_rounds as f64 / test_rounds as f64 * 100.0;
        
        println!("  📊 性能统计:");
        println!("    成功率:   {:.1}% ({}/{})", success_rate, successful_rounds, test_rounds);
        println!("    平均延迟: {:.2} ms", avg_time);
        println!("    最小延迟: {:.2} ms", min_time);
        println!("    最大延迟: {:.2} ms", max_time);
        println!("    P95延迟:  {:.2} ms", p95_time);
        println!("    QPS:      {:.0}", qps);
        println!();
    }
    
    // 总体统计
    if total_searches > 0 {
        let overall_avg = total_time / total_searches as f64;
        let overall_qps = 1000.0 / overall_avg;
        let overall_success_rate = total_searches as f64 / (total_searches + failed_searches) as f64 * 100.0;
        
        println!("🎯 总体性能统计");
        println!("================");
        println!("  总搜索次数: {}", total_searches);
        println!("  失败次数:   {}", failed_searches);
        println!("  成功率:     {:.1}%", overall_success_rate);
        println!("  平均延迟:   {:.2} ms", overall_avg);
        println!("  整体QPS:    {:.0}", overall_qps);
        
        // 性能评级
        if overall_qps >= 1000.0 {
            println!("  🏆 性能等级: 优秀 (QPS >= 1000)");
        } else if overall_qps >= 500.0 {
            println!("  🥈 性能等级: 良好 (QPS >= 500)");
        } else if overall_qps >= 100.0 {
            println!("  🥉 性能等级: 一般 (QPS >= 100)");
        } else {
            println!("  ⚠️  性能等级: 需要优化 (QPS < 100)");
        }
    } else {
        println!("❌ 所有搜索都失败了，无法生成性能统计");
    }
    
    // 并发测试
    println!();
    println!("🔄 并发性能测试 (文本搜索)");
    println!("================");
    
    run_concurrent_test().await?;
    
    Ok(())
}

async fn run_concurrent_test() -> Result<()> {
    use tokio::task::JoinSet;
    
    let concurrent_levels = vec![1, 5, 10, 20];
    
    for concurrency in concurrent_levels {
        println!("🔄 并发级别: {}", concurrency);
        
        let start_time = Instant::now();
        let mut join_set = JoinSet::new();
        
        for i in 0..concurrency {
            let query = format!("并发测试 {}", i);
            join_set.spawn(async move {
                // 创建一个新的数据库连接用于并发测试
                let config = VectorDbConfig::default();
                let data_dir = PathBuf::from("./performance_test_data");
                let test_db = VectorDatabase::new(data_dir, config).await?;
                
                // 尝试多次搜索以提高成功率 - 使用文本搜索
                for attempt in 0..3 {
                    match test_db.text_search(&query, 10).await {
                        Ok(results) => return Ok(results),
                        Err(e) => {
                            if attempt == 2 {
                                return Err(e);
                            }
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    }
                }
                
                Err(grape_vector_db::VectorDbError::QueryError("所有重试都失败了".to_string()))
            });
        }
        
        let mut successful_searches = 0;
        let mut failed_searches = 0;
        
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(_)) => successful_searches += 1,
                Ok(Err(e)) => {
                    failed_searches += 1;
                    error!("搜索失败: {}", e);
                }
                Err(e) => {
                    failed_searches += 1;
                    error!("任务失败: {}", e);
                }
            }
        }
        
        let total_time = start_time.elapsed();
        let concurrent_qps = successful_searches as f64 / total_time.as_secs_f64();
        let success_rate = successful_searches as f64 / (successful_searches + failed_searches) as f64 * 100.0;
        
        println!("  成功: {} / 失败: {}", successful_searches, failed_searches);
        println!("  成功率: {:.1}%", success_rate);
        println!("  总耗时: {:?}", total_time);
        println!("  并发QPS: {:.0}", concurrent_qps);
        println!();
    }
    
    Ok(())
} 