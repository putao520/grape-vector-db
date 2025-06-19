//! 内嵌模式完整示例
//! 
//! 这个示例展示了如何在应用程序中使用 Grape Vector Database 的内嵌模式，
//! 包括配置、生命周期管理、性能监控和故障处理。

use grape_vector_db::{
    embedded::{EmbeddedVectorDB, EmbeddedConfig, DatabaseState},
    advanced_storage::AdvancedStorageConfig,
    index::IndexConfig,
    types::{Point, Filter, Condition, FilterOperator},
    errors::Result,
};
use std::{collections::HashMap, time::Instant};
use tokio::time::{sleep, Duration};
use tracing::{info, warn, error, debug};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🍇 Grape Vector Database - 内嵌模式示例启动");

    // 运行各种示例场景
    basic_usage_example().await?;
    advanced_configuration_example().await?;
    batch_operations_example().await?;
    filtering_example().await?;
    performance_monitoring_example().await?;
    lifecycle_management_example().await?;

    info!("✅ 所有示例运行完成");
    Ok(())
}

/// 基本使用示例
async fn basic_usage_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("📖 示例1: 基本使用");

    // 1. 创建基本配置
    let config = EmbeddedConfig::default()
        .with_data_dir("./examples_data/basic")
        .with_vector_dimension(384)
        .with_memory_limit_mb(256);

    // 2. 初始化数据库
    let mut db = EmbeddedVectorDB::new(config).await?;
    info!("数据库初始化完成");

    // 3. 添加一些向量数据
    let points = vec![
        Point {
            id: "doc_1".to_string(),
            vector: generate_random_vector(384),
            payload: create_payload(&[
                ("title", "Rust编程语言"),
                ("category", "技术文档"),
                ("language", "zh"),
            ]),
        },
        Point {
            id: "doc_2".to_string(),
            vector: generate_random_vector(384),
            payload: create_payload(&[
                ("title", "机器学习基础"),
                ("category", "AI"),
                ("language", "zh"),
            ]),
        },
        Point {
            id: "doc_3".to_string(),
            vector: generate_random_vector(384),
            payload: create_payload(&[
                ("title", "向量数据库应用"),
                ("category", "数据库"),
                ("language", "zh"),
            ]),
        },
    ];

    // 插入向量
    for point in points {
        db.upsert_point(point).await?;
    }
    info!("已插入 3 个向量");

    // 4. 搜索相似向量
    let query_vector = generate_random_vector(384);
    let results = db.search_vectors(&query_vector, 5).await?;
    
    info!("搜索结果:");
    for (i, result) in results.iter().enumerate() {
        info!("  {}. ID: {}, 分数: {:.4}", i + 1, result.id, result.score);
    }

    // 5. 获取统计信息
    let stats = db.get_stats().await?;
    info!("数据库统计:");
    info!("  文档数量: {}", stats.document_count);
    info!("  内存使用: {:.2} MB", stats.memory_usage_mb);

    // 6. 关闭数据库
    db.close().await?;
    info!("基本使用示例完成\n");

    Ok(())
}

/// 高级配置示例
async fn advanced_configuration_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("⚙️ 示例2: 高级配置");

    // 创建高性能配置
    let config = EmbeddedConfig {
        data_dir: "./examples_data/advanced".into(),
        max_memory_mb: Some(512),
        thread_pool_size: Some(8),
        startup_timeout_ms: 30000,
        shutdown_timeout_ms: 10000,
        enable_warmup: true,
        vector_dimension: 768,
        storage: AdvancedStorageConfig {
            compression_enabled: true,
            cache_size_mb: 128,
            write_buffer_size_mb: 32,
            max_write_buffer_number: 3,
            target_file_size_mb: 64,
            bloom_filter_bits_per_key: 10,
            ..Default::default()
        },
        index: IndexConfig {
            m: 32,
            ef_construction: 400,
            ef_search: 200,
            max_m: 64,
            ml: 1.0 / (2.0_f32.ln()),
            ..Default::default()
        },
    };

    let start_time = Instant::now();
    let mut db = EmbeddedVectorDB::new(config).await?;
    let init_time = start_time.elapsed();
    
    info!("高级配置数据库初始化耗时: {:?}", init_time);

    // 添加更多数据来测试性能
    let mut points = Vec::new();
    for i in 0..100 {
        points.push(Point {
            id: format!("advanced_doc_{}", i),
            vector: generate_random_vector(768),
            payload: create_payload(&[
                ("index", &i.to_string()),
                ("category", if i % 3 == 0 { "tech" } else if i % 3 == 1 { "science" } else { "business" }),
                ("priority", &((i % 5) + 1).to_string()),
            ]),
        });
    }

    // 批量插入
    let insert_start = Instant::now();
    db.upsert_points_batch(points).await?;
    let insert_time = insert_start.elapsed();
    
    info!("批量插入100个向量耗时: {:?}", insert_time);

    // 测试搜索性能
    let search_start = Instant::now();
    let query = generate_random_vector(768);
    let results = db.search_vectors(&query, 10).await?;
    let search_time = search_start.elapsed();
    
    info!("搜索耗时: {:?}, 结果数量: {}", search_time, results.len());

    db.close().await?;
    info!("高级配置示例完成\n");

    Ok(())
}

/// 批量操作示例
async fn batch_operations_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("📊 示例3: 批量操作");

    let config = EmbeddedConfig::default()
        .with_data_dir("./examples_data/batch")
        .with_vector_dimension(512)
        .with_memory_limit_mb(512);

    let mut db = EmbeddedVectorDB::new(config).await?;

    // 准备大量数据
    let batch_size = 500;
    let mut all_points = Vec::new();
    
    for batch in 0..5 {
        let mut points = Vec::new();
        for i in 0..batch_size {
            points.push(Point {
                id: format!("batch_{}_{}", batch, i),
                vector: generate_random_vector(512),
                payload: create_payload(&[
                    ("batch_id", &batch.to_string()),
                    ("item_id", &i.to_string()),
                    ("type", "batch_data"),
                ]),
            });
        }
        
        let batch_start = Instant::now();
        db.upsert_points_batch(points.clone()).await?;
        let batch_time = batch_start.elapsed();
        
        info!("批次 {} 插入完成: {} 个向量, 耗时: {:?}", 
              batch, batch_size, batch_time);
        
        all_points.extend(points);
    }

    // 批量搜索测试
    let queries = vec![
        generate_random_vector(512),
        generate_random_vector(512),
        generate_random_vector(512),
    ];

    let batch_search_start = Instant::now();
    let batch_results = db.search_vectors_batch(&queries, 10).await?;
    let batch_search_time = batch_search_start.elapsed();

    info!("批量搜索完成: {} 个查询, 耗时: {:?}", 
          queries.len(), batch_search_time);

    for (i, results) in batch_results.iter().enumerate() {
        info!("  查询 {}: {} 个结果", i + 1, results.len());
    }

    // 获取最终统计
    let final_stats = db.get_stats().await?;
    info!("最终统计: {} 个文档, {:.2} MB 内存", 
          final_stats.document_count, final_stats.memory_usage_mb);

    db.close().await?;
    info!("批量操作示例完成\n");

    Ok(())
}

/// 过滤搜索示例
async fn filtering_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("🔍 示例4: 过滤搜索");

    let config = EmbeddedConfig::default()
        .with_data_dir("./examples_data/filtering")
        .with_vector_dimension(256);

    let mut db = EmbeddedVectorDB::new(config).await?;

    // 插入带有不同属性的向量
    let categories = ["技术", "商业", "科学", "艺术"];
    let priorities = [1, 2, 3, 4, 5];
    
    for i in 0..50 {
        let point = Point {
            id: format!("filtered_doc_{}", i),
            vector: generate_random_vector(256),
            payload: create_payload(&[
                ("category", categories[i % categories.len()]),
                ("priority", &priorities[i % priorities.len()].to_string()),
                ("score", &((i % 100) as f32 / 100.0).to_string()),
                ("active", if i % 3 == 0 { "true" } else { "false" }),
            ]),
        };
        db.upsert_point(point).await?;
    }

    let query = generate_random_vector(256);

    // 测试不同的过滤条件
    
    // 1. 单个条件过滤
    let filter1 = Filter {
        conditions: vec![
            Condition::Equals {
                field: "category".to_string(),
                value: "技术".into(),
            }
        ],
        operator: FilterOperator::And,
    };

    let results1 = db.search_vectors_with_filter(&query, 10, &filter1).await?;
    info!("技术类别过滤结果: {} 个", results1.len());

    // 2. 范围过滤
    let filter2 = Filter {
        conditions: vec![
            Condition::Range {
                field: "priority".to_string(),
                min: Some(3.into()),
                max: Some(5.into()),
            }
        ],
        operator: FilterOperator::And,
    };

    let results2 = db.search_vectors_with_filter(&query, 10, &filter2).await?;
    info!("高优先级过滤结果: {} 个", results2.len());

    // 3. 复合条件过滤
    let filter3 = Filter {
        conditions: vec![
            Condition::Equals {
                field: "active".to_string(),
                value: "true".into(),
            },
            Condition::Range {
                field: "score".to_string(),
                min: Some(0.5.into()),
                max: Some(1.0.into()),
            }
        ],
        operator: FilterOperator::And,
    };

    let results3 = db.search_vectors_with_filter(&query, 10, &filter3).await?;
    info!("活跃且高分过滤结果: {} 个", results3.len());

    // 4. OR条件过滤
    let filter4 = Filter {
        conditions: vec![
            Condition::Equals {
                field: "category".to_string(),
                value: "技术".into(),
            },
            Condition::Equals {
                field: "category".to_string(),
                value: "科学".into(),
            }
        ],
        operator: FilterOperator::Or,
    };

    let results4 = db.search_vectors_with_filter(&query, 10, &filter4).await?;
    info!("技术或科学类别过滤结果: {} 个", results4.len());

    db.close().await?;
    info!("过滤搜索示例完成\n");

    Ok(())
}

/// 性能监控示例
async fn performance_monitoring_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("📈 示例5: 性能监控");

    let config = EmbeddedConfig::default()
        .with_data_dir("./examples_data/monitoring")
        .with_vector_dimension(384)
        .with_memory_limit_mb(256);

    let mut db = EmbeddedVectorDB::new(config).await?;

    // 插入一些数据
    for i in 0..200 {
        let point = Point {
            id: format!("monitor_doc_{}", i),
            vector: generate_random_vector(384),
            payload: create_payload(&[
                ("id", &i.to_string()),
                ("group", &(i / 50).to_string()),
            ]),
        };
        db.upsert_point(point).await?;
    }

    // 定期监控性能指标
    for round in 1..=3 {
        info!("=== 监控轮次 {} ===", round);

        // 执行一些搜索操作
        for _ in 0..10 {
            let query = generate_random_vector(384);
            let _results = db.search_vectors(&query, 5).await?;
        }

        // 获取性能指标
        let metrics = db.get_metrics().await?;
        info!("性能指标:");
        info!("  查询延迟 P50: {:.2}ms", metrics.query_latency_p50);
        info!("  查询延迟 P95: {:.2}ms", metrics.query_latency_p95);
        info!("  查询延迟 P99: {:.2}ms", metrics.query_latency_p99);
        info!("  每秒查询数: {:.2}", metrics.queries_per_second);
        info!("  索引命中率: {:.2}%", metrics.index_hit_rate * 100.0);

        // 获取内存信息
        let memory_info = db.get_memory_info().await?;
        info!("内存使用:");
        info!("  总内存: {:.2} MB", memory_info.total_memory_mb);
        info!("  已用内存: {:.2} MB", memory_info.used_memory_mb);
        info!("  向量内存: {:.2} MB", memory_info.vector_memory_mb);
        info!("  索引内存: {:.2} MB", memory_info.index_memory_mb);
        info!("  缓存内存: {:.2} MB", memory_info.cache_memory_mb);

        // 检查是否需要内存清理
        if memory_info.used_memory_mb > 200.0 {
            warn!("内存使用过高，执行清理...");
            db.clear_cache().await?;
        }

        // 等待一段时间再进行下一轮监控
        sleep(Duration::from_secs(2)).await;
    }

    db.close().await?;
    info!("性能监控示例完成\n");

    Ok(())
}

/// 生命周期管理示例
async fn lifecycle_management_example() -> Result<(), Box<dyn std::error::Error>> {
    info!("🔄 示例6: 生命周期管理");

    let config = EmbeddedConfig::default()
        .with_data_dir("./examples_data/lifecycle")
        .with_vector_dimension(256)
        .with_startup_timeout(5000)
        .with_shutdown_timeout(3000);

    // 1. 正常启动
    info!("正常启动数据库...");
    let mut db = EmbeddedVectorDB::new(config.clone()).await?;
    
    // 检查状态
    let state = db.get_state().await;
    info!("启动后状态: {:?}", state);

    // 2. 健康检查
    let health = db.health_check().await?;
    info!("健康检查: {:?}", health);

    // 3. 添加一些数据
    for i in 0..20 {
        let point = Point {
            id: format!("lifecycle_doc_{}", i),
            vector: generate_random_vector(256),
            payload: create_payload(&[("id", &i.to_string())]),
        };
        db.upsert_point(point).await?;
    }

    // 4. 模拟故障恢复
    info!("模拟故障场景...");
    
    // 设置只读模式
    db.set_read_only(true).await?;
    info!("已设置为只读模式");

    // 尝试写入（应该失败）
    let point = Point {
        id: "readonly_test".to_string(),
        vector: generate_random_vector(256),
        payload: HashMap::new(),
    };
    
    match db.upsert_point(point).await {
        Ok(_) => warn!("只读模式下写入成功（不应该发生）"),
        Err(e) => info!("只读模式下写入失败（预期行为）: {}", e),
    }

    // 读取仍然可以工作
    let query = generate_random_vector(256);
    let results = db.search_vectors(&query, 5).await?;
    info!("只读模式下搜索成功: {} 个结果", results.len());

    // 恢复写入模式
    db.set_read_only(false).await?;
    info!("已恢复写入模式");

    // 5. 执行数据持久化
    info!("执行数据持久化...");
    db.flush().await?;
    info!("数据持久化完成");

    // 6. 优雅关闭
    info!("开始优雅关闭...");
    let shutdown_start = Instant::now();
    db.close().await?;
    let shutdown_time = shutdown_start.elapsed();
    info!("优雅关闭完成，耗时: {:?}", shutdown_time);

    // 7. 重新启动并验证数据持久性
    info!("重新启动数据库验证持久性...");
    let db2 = EmbeddedVectorDB::new(config).await?;
    
    let stats = db2.get_stats().await?;
    info!("重启后数据库统计: {} 个文档", stats.document_count);
    
    // 验证数据是否存在
    let query = generate_random_vector(256);
    let results = db2.search_vectors(&query, 5).await?;
    info!("重启后搜索结果: {} 个", results.len());

    db2.close().await?;
    info!("生命周期管理示例完成\n");

    Ok(())
}

/// 生成随机向量
fn generate_random_vector(dimension: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..dimension).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

/// 创建载荷数据
fn create_payload(fields: &[(&str, &str)]) -> HashMap<String, serde_json::Value> {
    fields.iter()
        .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
        .collect()
}

// 为了示例能够编译，我们需要提供一些trait实现的扩展方法
trait EmbeddedConfigExt {
    fn with_data_dir<P: Into<std::path::PathBuf>>(self, path: P) -> Self;
    fn with_vector_dimension(self, dim: usize) -> Self;
    fn with_memory_limit_mb(self, mb: usize) -> Self;
    fn with_startup_timeout(self, ms: u64) -> Self;
    fn with_shutdown_timeout(self, ms: u64) -> Self;
}

impl EmbeddedConfigExt for EmbeddedConfig {
    fn with_data_dir<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.data_dir = path.into();
        self
    }

    fn with_vector_dimension(mut self, dim: usize) -> Self {
        self.vector_dimension = dim;
        self
    }

    fn with_memory_limit_mb(mut self, mb: usize) -> Self {
        self.max_memory_mb = Some(mb);
        self
    }

    fn with_startup_timeout(mut self, ms: u64) -> Self {
        self.startup_timeout_ms = ms;
        self
    }

    fn with_shutdown_timeout(mut self, ms: u64) -> Self {
        self.shutdown_timeout_ms = ms;
        self
    }
}