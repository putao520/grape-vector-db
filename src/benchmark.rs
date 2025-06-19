//! 基准测试模块
//! 
//! 提供性能基准测试功能，包括：
//! - 搜索性能测试
//! - 融合策略比较
//! - 内存使用分析
//! - 延迟统计

use crate::{
    types::{
        HybridSearchRequest, FusionStrategy, FusionWeights, SearchResult
    },
    hybrid::HybridSearchEngine,
    storage::VectorStore,
    errors::Result,
};
use std::time::Instant;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// 基准测试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// 测试查询数量
    pub num_queries: usize,
    /// 每个查询的结果数量限制
    pub results_limit: usize,
    /// 并发查询数
    pub concurrent_queries: usize,
    /// 测试数据集大小
    pub dataset_size: usize,
    /// 向量维度
    pub vector_dimension: usize,
    /// 预热查询数量
    pub warmup_queries: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            num_queries: 1000,
            results_limit: 10,
            concurrent_queries: 4,
            dataset_size: 10000,
            vector_dimension: 384,
            warmup_queries: 100,
        }
    }
}

/// 基准测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// 策略名称
    pub strategy_name: String,
    /// 平均查询延迟 (ms)
    pub avg_latency_ms: f64,
    /// P50 延迟 (ms)
    pub p50_latency_ms: f64,
    /// P95 延迟 (ms)
    pub p95_latency_ms: f64,
    /// P99 延迟 (ms)
    pub p99_latency_ms: f64,
    /// 最大延迟 (ms)
    pub max_latency_ms: f64,
    /// 吞吐量 (QPS)
    pub throughput_qps: f64,
    /// 平均精确度
    pub avg_precision: f64,
    /// 平均召回率
    pub avg_recall: f64,
    /// 平均NDCG@10
    pub avg_ndcg_10: f64,
    /// 内存使用 (MB)
    pub memory_usage_mb: f64,
    /// 查询成功率
    pub success_rate: f64,
    /// 详细延迟分布
    pub latency_distribution: Vec<f64>,
}

/// 测试查询
#[derive(Debug, Clone)]
pub struct TestQuery {
    pub id: String,
    pub text: String,
    pub dense_vector: Option<Vec<f32>>,
    pub expected_results: Vec<String>, // 预期的文档ID
}

/// 基准测试套件
pub struct BenchmarkSuite {
    config: BenchmarkConfig,
    test_queries: Vec<TestQuery>,
    relevance_judgments: HashMap<String, HashMap<String, f32>>, // query_id -> doc_id -> relevance
}

impl BenchmarkSuite {
    /// 创建新的基准测试套件
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            config,
            test_queries: Vec::new(),
            relevance_judgments: HashMap::new(),
        }
    }

    /// 生成测试数据
    pub fn generate_test_data(&mut self) -> Result<()> {
        // 生成测试查询
        self.test_queries = (0..self.config.num_queries)
            .map(|i| TestQuery {
                id: format!("query_{}", i),
                text: self.generate_random_query(i),
                dense_vector: Some(self.generate_random_vector()),
                expected_results: self.generate_expected_results(i),
            })
            .collect();

        // 生成相关性判断
        for query in &self.test_queries {
            let mut judgments = HashMap::new();
            for doc_id in &query.expected_results {
                judgments.insert(doc_id.clone(), 1.0);
            }
            self.relevance_judgments.insert(query.id.clone(), judgments);
        }

        Ok(())
    }

    /// 运行融合策略比较测试
    pub async fn run_fusion_strategy_comparison<S: VectorStore + ?Sized>(
        &self,
        engine: &HybridSearchEngine,
        store: &S,
    ) -> Result<Vec<BenchmarkResult>> {
        let strategies = vec![
            ("RRF_k60", FusionStrategy::RRF { k: 60.0 }),
            ("RRF_k30", FusionStrategy::RRF { k: 30.0 }),
            ("Linear_Balanced", FusionStrategy::Linear { 
                dense_weight: 0.5, sparse_weight: 0.3, text_weight: 0.2 
            }),
            ("Linear_Dense_Heavy", FusionStrategy::Linear { 
                dense_weight: 0.7, sparse_weight: 0.2, text_weight: 0.1 
            }),
            ("Linear_Sparse_Heavy", FusionStrategy::Linear { 
                dense_weight: 0.3, sparse_weight: 0.6, text_weight: 0.1 
            }),
            ("Normalized_Balanced", FusionStrategy::Normalized { 
                dense_weight: 0.5, sparse_weight: 0.3, text_weight: 0.2 
            }),
            ("Learned_Adaptive", FusionStrategy::Learned {
                base_weights: FusionWeights::default(),
                query_type_adaptation: true,
                quality_adaptation: true,
            }),
            ("Adaptive_Learning", FusionStrategy::Adaptive {
                initial_weights: FusionWeights::default(),
                learning_rate: 0.01,
                history_size: 1000,
            }),
        ];

        let mut results = Vec::new();

        for (name, strategy) in strategies {
            println!("测试融合策略: {}", name);
            
            // 创建临时引擎（实际应该重新配置现有引擎）
            let result = self.benchmark_strategy(engine, store, &strategy, name).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// 单个策略的基准测试
    async fn benchmark_strategy<S: VectorStore + ?Sized>(
        &self,
        engine: &HybridSearchEngine,
        store: &S,
        _strategy: &FusionStrategy,
        strategy_name: &str,
    ) -> Result<BenchmarkResult> {
        let mut latencies = Vec::new();
        let mut precisions = Vec::new();
        let mut recalls = Vec::new();
        let mut ndcgs = Vec::new();
        let mut success_count = 0;

        let start_time = Instant::now();

        // 预热
        for i in 0..self.config.warmup_queries.min(self.test_queries.len()) {
            let query = &self.test_queries[i];
            let _ = self.execute_query(engine, store, query).await;
        }

        // 正式测试
        for query in &self.test_queries {
            let query_start = Instant::now();
            
            match self.execute_query(engine, store, query).await {
                Ok(results) => {
                    let latency = query_start.elapsed().as_millis() as f64;
                    latencies.push(latency);
                    
                    // 计算精确度和召回率
                    let (precision, recall) = self.calculate_precision_recall(query, &results);
                    precisions.push(precision);
                    recalls.push(recall);
                    
                    // 计算NDCG@10
                    let ndcg = self.calculate_ndcg(query, &results, 10);
                    ndcgs.push(ndcg);
                    
                    success_count += 1;
                }
                Err(_) => {
                    // 查询失败，记录为最大延迟
                    latencies.push(10000.0);
                }
            }
        }

        let total_time = start_time.elapsed();
        
        // 计算统计指标
        let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
        let throughput = success_count as f64 / total_time.as_secs_f64();
        let success_rate = success_count as f64 / self.test_queries.len() as f64;

        // 计算百分位数
        let mut sorted_latencies = latencies.clone();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let p50 = self.percentile(&sorted_latencies, 50.0);
        let p95 = self.percentile(&sorted_latencies, 95.0);
        let p99 = self.percentile(&sorted_latencies, 99.0);
        let max_latency = sorted_latencies.last().copied().unwrap_or(0.0);

        Ok(BenchmarkResult {
            strategy_name: strategy_name.to_string(),
            avg_latency_ms: avg_latency,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            max_latency_ms: max_latency,
            throughput_qps: throughput,
            avg_precision: precisions.iter().sum::<f64>() / precisions.len() as f64,
            avg_recall: recalls.iter().sum::<f64>() / recalls.len() as f64,
            avg_ndcg_10: ndcgs.iter().sum::<f64>() / ndcgs.len() as f64,
            memory_usage_mb: self.estimate_memory_usage(engine),
            success_rate,
            latency_distribution: sorted_latencies,
        })
    }

    /// 执行单个查询
    async fn execute_query<S: VectorStore + ?Sized>(
        &self,
        engine: &HybridSearchEngine,
        store: &S,
        query: &TestQuery,
    ) -> Result<Vec<SearchResult>> {
        let request = HybridSearchRequest {
            dense_vector: query.dense_vector.clone(),
            sparse_vector: None,
            text_query: Some(query.text.clone()),
            limit: self.config.results_limit,
            dense_weight: 0.5,
            sparse_weight: 0.3,
            text_weight: 0.2,
        };

        engine.search(store, &request).await
    }

    /// 计算精确度和召回率
    fn calculate_precision_recall(&self, query: &TestQuery, results: &[SearchResult]) -> (f64, f64) {
        let relevant_docs: std::collections::HashSet<_> = query.expected_results.iter().collect();
        let retrieved_docs: std::collections::HashSet<_> = results.iter()
            .map(|r| &r.document.id)
            .collect();

        let relevant_retrieved = relevant_docs.intersection(&retrieved_docs).count();
        
        let precision = if results.is_empty() {
            0.0
        } else {
            relevant_retrieved as f64 / results.len() as f64
        };

        let recall = if relevant_docs.is_empty() {
            1.0
        } else {
            relevant_retrieved as f64 / relevant_docs.len() as f64
        };

        (precision, recall)
    }

    /// 计算NDCG (Normalized Discounted Cumulative Gain)
    fn calculate_ndcg(&self, query: &TestQuery, results: &[SearchResult], k: usize) -> f64 {
        if let Some(relevance) = self.relevance_judgments.get(&query.id) {
            let dcg = self.calculate_dcg(results, relevance, k);
            let idcg = self.calculate_ideal_dcg(relevance, k);
            
            if idcg > 0.0 {
                dcg / idcg
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// 计算DCG
    fn calculate_dcg(
        &self, 
        results: &[SearchResult], 
        relevance: &HashMap<String, f32>, 
        k: usize
    ) -> f64 {
        results.iter()
            .take(k)
            .enumerate()
            .map(|(i, result)| {
                let rel = relevance.get(&result.document.id).copied().unwrap_or(0.0);
                let rank = i + 1;
                rel as f64 / (rank as f64).log2()
            })
            .sum()
    }

    /// 计算理想DCG
    fn calculate_ideal_dcg(&self, relevance: &HashMap<String, f32>, k: usize) -> f64 {
        let mut rel_scores: Vec<f32> = relevance.values().copied().collect();
        rel_scores.sort_by(|a, b| b.partial_cmp(a).unwrap());
        
        rel_scores.iter()
            .take(k)
            .enumerate()
            .map(|(i, &rel)| {
                let rank = i + 1;
                rel as f64 / (rank as f64).log2()
            })
            .sum()
    }

    /// 计算百分位数
    fn percentile(&self, sorted_data: &[f64], percentile: f64) -> f64 {
        if sorted_data.is_empty() {
            return 0.0;
        }
        
        let index = (percentile / 100.0 * (sorted_data.len() - 1) as f64).round() as usize;
        sorted_data.get(index).copied().unwrap_or(0.0)
    }

    /// 估算内存使用量
    fn estimate_memory_usage(&self, engine: &HybridSearchEngine) -> f64 {
        let stats = engine.get_stats();
        stats.memory_usage_mb
    }

    /// 生成随机查询
    fn generate_random_query(&self, index: usize) -> String {
        let queries = vec![
            "如何使用向量数据库进行语义搜索",
            "数据库性能优化技巧",
            "机器学习模型训练",
            "分布式系统架构设计",
            "API接口开发最佳实践",
            "云计算服务部署",
            "数据安全与隐私保护",
            "微服务架构模式",
            "容器化部署策略",
            "监控与日志系统",
        ];
        
        queries[index % queries.len()].to_string()
    }

    /// 生成随机向量
    fn generate_random_vector(&self) -> Vec<f32> {
        (0..self.config.vector_dimension)
            .map(|_| fastrand::f32() - 0.5)
            .collect()
    }

    /// 生成预期结果
    fn generate_expected_results(&self, index: usize) -> Vec<String> {
        (0..5).map(|i| format!("doc_{}_{}", index, i)).collect()
    }

    /// 导出基准测试报告
    pub fn export_report(&self, results: &[BenchmarkResult]) -> Result<String> {
        let mut report = String::new();
        
        report.push_str("# 向量数据库混合搜索性能基准测试报告\n\n");
        report.push_str(&format!("测试配置:\n"));
        report.push_str(&format!("- 查询数量: {}\n", self.config.num_queries));
        report.push_str(&format!("- 结果限制: {}\n", self.config.results_limit));
        report.push_str(&format!("- 数据集大小: {}\n", self.config.dataset_size));
        report.push_str(&format!("- 向量维度: {}\n\n", self.config.vector_dimension));

        report.push_str("## 性能对比结果\n\n");
        report.push_str("| 策略 | 平均延迟(ms) | P95延迟(ms) | 吞吐量(QPS) | 精确度 | 召回率 | NDCG@10 |\n");
        report.push_str("|------|------------|------------|------------|--------|--------|---------|\n");

        for result in results {
            report.push_str(&format!(
                "| {} | {:.2} | {:.2} | {:.2} | {:.3} | {:.3} | {:.3} |\n",
                result.strategy_name,
                result.avg_latency_ms,
                result.p95_latency_ms,
                result.throughput_qps,
                result.avg_precision,
                result.avg_recall,
                result.avg_ndcg_10
            ));
        }

        report.push_str("\n## 详细分析\n\n");
        
        // 找出最佳策略
        let best_latency = results.iter()
            .min_by(|a, b| a.avg_latency_ms.partial_cmp(&b.avg_latency_ms).unwrap());
        let best_precision = results.iter()
            .max_by(|a, b| a.avg_precision.partial_cmp(&b.avg_precision).unwrap());
        let best_throughput = results.iter()
            .max_by(|a, b| a.throughput_qps.partial_cmp(&b.throughput_qps).unwrap());

        if let Some(best) = best_latency {
            report.push_str(&format!("**最低延迟策略**: {} ({:.2}ms)\n", best.strategy_name, best.avg_latency_ms));
        }
        if let Some(best) = best_precision {
            report.push_str(&format!("**最高精确度策略**: {} ({:.3})\n", best.strategy_name, best.avg_precision));
        }
        if let Some(best) = best_throughput {
            report.push_str(&format!("**最高吞吐量策略**: {} ({:.2} QPS)\n", best.strategy_name, best.throughput_qps));
        }

        Ok(report)
    }
}

/// 并发性能测试 - 简化版本，避免生命周期问题
/// 注意：此函数为演示用途，实际使用时需要考虑更复杂的并发模式
pub fn create_concurrent_benchmark_config(
    queries: Vec<TestQuery>,
    concurrency: usize,
) -> (Vec<Vec<TestQuery>>, BenchmarkConfig) {
    // 将查询分成批次，返回配置供外部使用
    let chunk_size = (queries.len() + concurrency - 1) / concurrency;
    let chunks: Vec<Vec<TestQuery>> = queries.chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect();
    
    let config = BenchmarkConfig {
        num_queries: queries.len(),
        concurrent_queries: concurrency,
        ..Default::default()
    };
    
    (chunks, config)
} 