// Binary Quantization 演示程序
// Week 5-6: Binary Quantization Implementation

use grape_vector_db::{
    quantization::{BinaryQuantizer, BinaryQuantizationConfig, BinaryVectorStore, QuantizationMetrics},
    errors::Result,
};
use std::time::Instant;
use fastrand::Rng;

fn main() -> Result<()> {
    println!("🚀 Binary Quantization 演示程序");
    println!("=====================================");
    
    // 1. 配置和初始化
    let config = BinaryQuantizationConfig {
        threshold: 0.0,
        enable_simd: true,
        rescore_ratio: 0.2,
        enable_cache: true,
    };
    
    let mut quantizer = BinaryQuantizer::new(config.clone());
    let mut store = BinaryVectorStore::new(config);
    
    // 2. 生成测试数据
    println!("\n📊 生成测试数据...");
    let dimension = 512;
    let num_vectors = 10_000;
    let num_queries = 100;
    
    let mut rng = Rng::new();
    
    // 生成数据集
    let mut dataset: Vec<Vec<f32>> = Vec::new();
    for _i in 0..num_vectors {
        let vector: Vec<f32> = (0..dimension)
            .map(|_| rng.f32() * 2.0 - 1.0) // [-1.0, 1.0]
            .collect();
        dataset.push(vector);
    }
    
    // 生成查询向量
    let queries: Vec<Vec<f32>> = (0..num_queries)
        .map(|_| {
            (0..dimension)
                .map(|_| rng.f32() * 2.0 - 1.0)
                .collect()
        })
        .collect();
    
    println!("✅ 已生成 {} 个 {}-维向量", num_vectors, dimension);
    println!("✅ 已生成 {} 个查询向量", num_queries);
    
    // 3. Binary Quantization 性能测试
    println!("\n⚡ Binary Quantization 性能测试");
    println!("-------------------------------");
    
    // 量化数据集
    let start = Instant::now();
    let mut binary_dataset = Vec::new();
    for (i, vector) in dataset.iter().enumerate() {
        let binary_vec = quantizer.quantize(vector)?;
        store.add_vector(binary_vec.clone(), format!("doc_{}", i))?;
        binary_dataset.push(binary_vec);
    }
    let quantization_time = start.elapsed();
    
    println!("⏱️  量化时间: {:?}", quantization_time);
    println!("📦 内存使用: {} KB", store.memory_usage() / 1024);
    
    // 计算压缩比
    let original_size = num_vectors * dimension * 4; // 4 bytes per f32
    let quantized_size = store.memory_usage();
    let compression_ratio = original_size as f32 / quantized_size as f32;
    println!("🗜️  压缩比: {:.1}x (原始: {} KB → 量化: {} KB)", 
             compression_ratio, original_size / 1024, quantized_size / 1024);
    
    // 4. 搜索性能对比
    println!("\n🔍 搜索性能对比");
    println!("----------------");
    
    // Binary搜索测试
    let start = Instant::now();
    let mut binary_results = Vec::new();
    
    for query in &queries {
        let query_binary = quantizer.quantize(query)?;
        
        // 找到最相似的5个向量
        let mut similarities: Vec<(usize, f32)> = Vec::new();
        for (idx, candidate) in binary_dataset.iter().enumerate() {
            if let Ok(sim) = quantizer.similarity(&query_binary, candidate) {
                similarities.push((idx, sim));
            }
        }
        
        // 排序并取前5个
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        similarities.truncate(5);
        binary_results.push(similarities);
    }
    
    let binary_search_time = start.elapsed();
    
    // 精确搜索测试（用于对比）
    let start = Instant::now();
    let mut exact_results = Vec::new();
    
    for query in &queries {
        let mut similarities: Vec<(usize, f32)> = Vec::new();
        for (idx, candidate) in dataset.iter().enumerate() {
            let sim = cosine_similarity(query, candidate);
            similarities.push((idx, sim));
        }
        
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        similarities.truncate(5);
        exact_results.push(similarities);
    }
    
    let exact_search_time = start.elapsed();
    
    // 性能对比
    let speedup = exact_search_time.as_millis() as f32 / binary_search_time.as_millis() as f32;
    println!("⚡ Binary搜索时间: {:?}", binary_search_time);
    println!("🐌 精确搜索时间: {:?}", exact_search_time);
    println!("🚀 加速比: {:.1}x", speedup);
    
    // 5. 准确性评估
    println!("\n📊 准确性评估");
    println!("-------------");
    
    let mut recall_scores = Vec::new();
    for i in 0..num_queries {
        let binary_top5: std::collections::HashSet<usize> = binary_results[i]
            .iter()
            .map(|(idx, _)| *idx)
            .collect();
        let exact_top5: std::collections::HashSet<usize> = exact_results[i]
            .iter()
            .map(|(idx, _)| *idx)
            .collect();
        
        let intersection = binary_top5.intersection(&exact_top5).count();
        let recall = intersection as f32 / exact_top5.len() as f32;
        recall_scores.push(recall);
    }
    
    let avg_recall = recall_scores.iter().sum::<f32>() / recall_scores.len() as f32;
    println!("📈 平均 Recall@5: {:.3}", avg_recall);
    
    // 6. 多阶段搜索演示
    println!("\n🎯 多阶段搜索演示");
    println!("------------------");
    
    if !queries.is_empty() && !dataset.is_empty() {
        let query = &queries[0];
        let query_binary = quantizer.quantize(query)?;
        
        let start = Instant::now();
        let multi_stage_results = quantizer.multi_stage_search(
            &query_binary,
            &binary_dataset,
            query,
            &dataset,
        )?;
        let multi_stage_time = start.elapsed();
        
        println!("⏱️  多阶段搜索时间: {:?}", multi_stage_time);
        println!("🎯 返回结果数: {}", multi_stage_results.len());
        
        if !multi_stage_results.is_empty() {
            println!("📊 Top-3 结果:");
            for (i, (idx, score)) in multi_stage_results.iter().take(3).enumerate() {
                println!("   {}. 索引: {}, 相似度: {:.4}", i + 1, idx, score);
            }
        }
    }
    
    // 7. 综合性能指标
    println!("\n📈 综合性能报告");
    println!("=================");
    
    let metrics = QuantizationMetrics {
        quantization_time_ms: quantization_time.as_millis() as f64,
        search_time_ms: binary_search_time.as_millis() as f64,
        memory_usage_bytes: quantized_size,
        compression_ratio,
        accuracy: avg_recall,
    };
    
    println!("🔢 性能指标:");
    println!("   • 量化时间: {:.1} ms", metrics.quantization_time_ms);
    println!("   • 搜索时间: {:.1} ms", metrics.search_time_ms);
    println!("   • 内存使用: {} KB", metrics.memory_usage_bytes / 1024);
    println!("   • 压缩比: {:.1}x", metrics.compression_ratio);
    println!("   • 准确率: {:.1}%", metrics.accuracy * 100.0);
    
    let throughput = num_queries as f64 / (metrics.search_time_ms / 1000.0);
    println!("   • 查询吞吐量: {:.0} QPS", throughput);
    
    println!("\n✅ Binary Quantization 演示完成!");
    println!("🎉 实现了 {:.1}x 内存压缩和 {:.1}x 搜索加速！", compression_ratio, speedup);
    
    Ok(())
}

/// 计算余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
} 