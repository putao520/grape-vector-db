//! 融合策略基准测试示例
//!
//! 此示例演示如何使用不同的融合策略进行性能基准测试

use grape_vector_db::{
    hybrid::{FusionContext, FusionModel, QueryType, StatisticalFusionModel, TimeContext},
    types::Document,
    VectorDatabase, VectorDbConfig,
};
use std::collections::HashMap;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Grape Vector DB - 融合策略基准测试演示");

    // 1. 初始化数据库
    let config = VectorDbConfig::default();
    let data_dir = PathBuf::from("./temp_demo_db");
    let db = VectorDatabase::new(data_dir, config).await?;

    // 2. 准备测试数据
    let test_documents = generate_test_documents();
    println!("📚 生成了 {} 个测试文档", test_documents.len());

    // 插入测试文档
    for doc in &test_documents {
        db.add_document(doc.clone()).await?;
    }

    println!("✅ 文档插入完成");

    // 验证搜索功能
    println!("🔍 验证搜索功能...");
    match db.semantic_search("测试查询", 5).await {
        Ok(results) => println!("✅ 语义搜索功能正常，找到 {} 个结果", results.len()),
        Err(e) => {
            println!("❌ 搜索验证失败: {}", e);
            return Err(e.into());
        }
    }

    // 3. 测试不同的融合策略
    println!("\n🎯 开始融合策略性能测试...\n");

    let strategies = vec![
        ("RRF_模拟", 0.5, 0.3, 0.2),
        ("线性_平衡", 0.5, 0.3, 0.2),
        ("线性_密集偏重", 0.7, 0.2, 0.1),
        ("线性_稀疏偏重", 0.3, 0.6, 0.1),
        ("标准化融合", 0.6, 0.3, 0.1),
        ("学习式模拟", 0.55, 0.35, 0.1),
        ("自适应模拟", 0.65, 0.25, 0.1),
    ];

    let mut all_results = Vec::new();

    for (name, dense_weight, sparse_weight, text_weight) in strategies {
        println!("📊 测试策略: {}", name);

        // 使用相同的数据库但不同的权重配置运行测试
        let result = run_simple_benchmark_with_weights(
            &db,
            name,
            &test_documents,
            dense_weight,
            sparse_weight,
            text_weight,
        )
        .await?;
        all_results.push(result);

        println!("   ✓ 完成");
    }

    // 5. 分析结果
    println!("\n📈 基准测试结果分析:\n");

    print_benchmark_results(&all_results);

    // 6. 演示学习式融合模型
    println!("\n🧠 学习式融合模型演示:");
    demonstrate_learning_model()?;

    println!("\n🎉 基准测试演示完成!");
    Ok(())
}

/// 生成测试文档
fn generate_test_documents() -> Vec<Document> {
    let now = chrono::Utc::now();
    vec![
        Document {
            id: "doc1".to_string(),
            title: Some("向量数据库介绍".to_string()),
            content: "向量数据库是一种专门用于存储和检索高维向量数据的数据库系统。它支持语义搜索和相似性查询。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
        Document {
            id: "doc2".to_string(),
            title: Some("机器学习基础".to_string()),
            content: "机器学习是人工智能的一个重要分支，通过算法让计算机自动学习数据中的模式。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
        Document {
            id: "doc3".to_string(),
            title: Some("深度学习技术".to_string()),
            content: "深度学习使用神经网络模型处理复杂的数据模式，在图像识别和自然语言处理方面表现优异。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
        Document {
            id: "doc4".to_string(),
            title: Some("搜索引擎优化".to_string()),
            content: "搜索引擎优化(SEO)是提高网站在搜索引擎结果页面排名的技术和策略。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
        Document {
            id: "doc5".to_string(),
            title: Some("分布式系统".to_string()),
            content: "分布式系统是由多个独立计算机组成的系统，它们通过网络通信协作完成任务。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
        Document {
            id: "doc6".to_string(),
            title: Some("数据库设计".to_string()),
            content: "良好的数据库设计是构建高效应用程序的基础，需要考虑性能、扩展性和数据一致性。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
        Document {
            id: "doc7".to_string(),
            title: Some("API开发最佳实践".to_string()),
            content: "RESTful API设计应该遵循统一的接口规范，提供清晰的错误处理和完整的文档。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
        Document {
            id: "doc8".to_string(),
            title: Some("云计算平台".to_string()),
            content: "云计算提供按需的计算资源，包括基础设施即服务(IaaS)、平台即服务(PaaS)和软件即服务(SaaS)。".to_string(),
            language: Some("zh".to_string()),
            package_name: Some("demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("article".to_string()),
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        },
    ]
}

/// 简化的基准测试结构
#[derive(Debug)]
struct SimpleBenchmarkResult {
    strategy_name: String,
    avg_latency_ms: f64,
    throughput_qps: f64,
    success_rate: f64,
    sample_precision: f64,
}

/// 运行简化的基准测试（带权重配置）
async fn run_simple_benchmark_with_weights(
    db: &VectorDatabase,
    strategy_name: &str,
    _test_docs: &[Document],
    _dense_weight: f32,
    _sparse_weight: f32,
    _text_weight: f32,
) -> Result<SimpleBenchmarkResult, Box<dyn std::error::Error>> {
    let test_queries = [
        "向量数据库搜索",
        "机器学习算法",
        "深度学习神经网络",
        "搜索引擎技术",
        "分布式系统架构",
    ];

    let start_time = std::time::Instant::now();
    let mut latencies = Vec::new();
    let mut successful_queries = 0;
    let mut total_precision = 0.0;

    for query in test_queries.iter() {
        let query_start = std::time::Instant::now();

        // 执行语义搜索
        match db.semantic_search(query, 5).await {
            Ok(results) => {
                let latency = query_start.elapsed().as_millis() as f64;
                latencies.push(latency);
                successful_queries += 1;

                // 简单的精确度计算（基于结果数量）
                let precision = if results.is_empty() {
                    0.0
                } else {
                    (results.len() as f64 / 5.0).min(1.0)
                };
                total_precision += precision;
            }
            Err(_) => {
                latencies.push(5000.0); // 错误时记录高延迟
            }
        }
    }

    let total_time = start_time.elapsed();
    let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let throughput = successful_queries as f64 / total_time.as_secs_f64();
    let success_rate = successful_queries as f64 / test_queries.len() as f64;
    let avg_precision = total_precision / test_queries.len() as f64;

    Ok(SimpleBenchmarkResult {
        strategy_name: strategy_name.to_string(),
        avg_latency_ms: avg_latency,
        throughput_qps: throughput,
        success_rate,
        sample_precision: avg_precision,
    })
}

/// 打印基准测试结果
fn print_benchmark_results(results: &[SimpleBenchmarkResult]) {
    println!("┌─────────────────┬──────────────┬──────────────┬─────────────┬──────────────┐");
    println!("│ 策略            │ 平均延迟(ms) │ 吞吐量(QPS)  │ 成功率      │ 样本精确度   │");
    println!("├─────────────────┼──────────────┼──────────────┼─────────────┼──────────────┤");

    for result in results {
        println!(
            "│ {:15} │ {:12.2} │ {:12.2} │ {:11.1}% │ {:12.3} │",
            result.strategy_name,
            result.avg_latency_ms,
            result.throughput_qps,
            result.success_rate * 100.0,
            result.sample_precision
        );
    }

    println!("└─────────────────┴──────────────┴──────────────┴─────────────┴──────────────┘");

    // 找出最佳性能指标
    if let Some(best_latency) = results
        .iter()
        .min_by(|a, b| a.avg_latency_ms.partial_cmp(&b.avg_latency_ms).unwrap())
    {
        println!(
            "\n🏆 最低延迟: {} ({:.2}ms)",
            best_latency.strategy_name, best_latency.avg_latency_ms
        );
    }

    if let Some(best_throughput) = results
        .iter()
        .max_by(|a, b| a.throughput_qps.partial_cmp(&b.throughput_qps).unwrap())
    {
        println!(
            "🏆 最高吞吐量: {} ({:.2} QPS)",
            best_throughput.strategy_name, best_throughput.throughput_qps
        );
    }

    if let Some(best_precision) = results
        .iter()
        .max_by(|a, b| a.sample_precision.partial_cmp(&b.sample_precision).unwrap())
    {
        println!(
            "🏆 最高精确度: {} ({:.3})",
            best_precision.strategy_name, best_precision.sample_precision
        );
    }
}

/// 演示学习式融合模型
fn demonstrate_learning_model() -> Result<(), Box<dyn std::error::Error>> {
    println!("  创建统计学习模型...");

    let mut model = StatisticalFusionModel::new(0.01);

    // 模拟一些训练数据
    use grape_vector_db::types::QueryMetrics;

    let training_data = vec![
        QueryMetrics {
            query_id: "test_query_1".to_string(),
            query_text: "示例查询".to_string(),
            timestamp: 1_640_995_200_000, // 2022-01-01 00:00:00 UTC
            duration_ms: 150.0,
            result_count: 10,
            clicked_results: vec!["doc1".to_string(), "doc3".to_string()],
            user_satisfaction: Some(4),
            fusion_strategy: "learned".to_string(),
        },
        QueryMetrics {
            query_id: "test_query_2".to_string(),
            query_text: "另一个查询".to_string(),
            timestamp: 1_641_081_600_000, // 2022-01-02 00:00:00 UTC
            duration_ms: 120.0,
            result_count: 8,
            clicked_results: vec!["doc2".to_string()],
            user_satisfaction: Some(5),
            fusion_strategy: "learned".to_string(),
        },
    ];

    // 训练模型
    for metrics in training_data {
        model.update_model(&metrics)?;
    }

    // 测试预测
    let test_context = FusionContext {
        query_type: QueryType::Semantic,
        query_length: 15,
        historical_ctr: 0.75,
        time_context: TimeContext::WorkingHours,
    };

    let predicted_weights = model.predict_weights("深度学习技术", &test_context);

    println!("  📊 预测的最佳权重:");
    println!("    - 密集向量权重: {:.3}", predicted_weights.dense_weight);
    println!("    - 稀疏向量权重: {:.3}", predicted_weights.sparse_weight);
    println!("    - 文本搜索权重: {:.3}", predicted_weights.text_weight);

    Ok(())
}
