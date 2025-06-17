/// 测试工具函数
/// 
/// 提供通用的测试辅助函数和数据生成器

use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;

use grape_vector_db::types::*;

/// 生成测试向量
pub fn generate_test_vector(id: usize) -> Vec<f32> {
    (0..128).map(|i| (id * 128 + i) as f32 / 1000.0).collect()
}

/// 生成随机测试向量
pub fn generate_random_vector(dimension: usize) -> Vec<f32> {
    (0..dimension).map(|_| fastrand::f32() * 2.0 - 1.0).collect()
}

/// 创建测试文档
pub fn create_test_document_with_id(id: &str) -> Document {
    Document {
        id: id.to_string(),
        title: Some(format!("测试文档 {}", id)),
        content: format!("这是测试文档{}的内容", id),
        language: Some("zh".to_string()),
        version: Some("1".to_string()),
        doc_type: Some("test".to_string()),
        package_name: Some("test_package".to_string()),
        vector: Some(generate_test_vector(id.len())),
        metadata: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// 创建带有自定义向量的测试文档
pub fn create_test_document_with_vector(id: &str, vector: Vec<f32>) -> Document {
    Document {
        id: id.to_string(),
        title: Some(format!("测试文档 {}", id)),
        content: format!("这是测试文档{}的内容", id),
        language: Some("zh".to_string()),
        version: Some("1".to_string()),
        doc_type: Some("test".to_string()),
        package_name: Some("test_package".to_string()),
        vector: Some(vector),
        metadata: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// 生成测试文档批次
pub fn generate_test_document_batch(count: usize) -> Vec<Document> {
    (0..count)
        .map(|i| create_test_document_with_id(&format!("doc_{}", i)))
        .collect()
}

/// 创建具有特定属性的测试文档
pub fn create_test_document_with_metadata(
    id: &str,
    metadata: HashMap<String, String>,
) -> Document {
    Document {
        id: id.to_string(),
        title: Some(format!("测试文档 {}", id)),
        content: format!("这是测试文档{}的内容", id),
        language: Some("zh".to_string()),
        version: Some("1".to_string()),
        doc_type: Some("test".to_string()),
        package_name: Some("test_package".to_string()),
        vector: Some(generate_test_vector(id.len())),
        metadata,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// 验证搜索结果的有效性
pub fn validate_search_results(results: &[SearchResult], expected_count: usize) -> bool {
    if results.len() != expected_count {
        return false;
    }
    
    // 验证分数是否按降序排列
    for i in 1..results.len() {
        if results[i-1].score < results[i].score {
            return false;
        }
    }
    
    // 验证分数范围 [0.0, 1.0]
    for result in results {
        if result.score < 0.0 || result.score > 1.0 {
            return false;
        }
    }
    
    true
}

/// 比较两个浮点数是否近似相等
pub fn approximately_equal(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() < epsilon
}

/// 比较两个向量是否近似相等
pub fn vectors_approximately_equal(a: &[f32], b: &[f32], epsilon: f32) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    a.iter()
        .zip(b.iter())
        .all(|(&x, &y)| approximately_equal(x, y, epsilon))
}

/// 计算向量的余弦相似度
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    
    dot_product / (norm_a * norm_b)
}

/// 等待指定条件成立，带超时
pub async fn wait_for_condition<F, Fut>(
    condition: F,
    timeout: std::time::Duration,
    check_interval: std::time::Duration,
) -> bool
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout {
        if condition().await {
            return true;
        }
        tokio::time::sleep(check_interval).await;
    }
    
    false
}

/// 性能测试辅助函数
pub struct PerformanceTester {
    start_time: std::time::Instant,
    operation_count: usize,
}

impl PerformanceTester {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            operation_count: 0,
        }
    }
    
    pub fn record_operation(&mut self) {
        self.operation_count += 1;
    }
    
    pub fn get_throughput(&self) -> f64 {
        if self.operation_count == 0 {
            return 0.0;
        }
        
        let elapsed_secs = self.start_time.elapsed().as_secs_f64();
        if elapsed_secs == 0.0 {
            return 0.0;
        }
        
        self.operation_count as f64 / elapsed_secs
    }
    
    pub fn get_average_latency(&self) -> std::time::Duration {
        if self.operation_count == 0 {
            return std::time::Duration::ZERO;
        }
        
        self.start_time.elapsed() / self.operation_count as u32
    }
}

/// 负载均衡验证器
pub struct LoadBalanceValidator {
    node_counts: HashMap<String, usize>,
    total_operations: usize,
}

impl LoadBalanceValidator {
    pub fn new() -> Self {
        Self {
            node_counts: HashMap::new(),
            total_operations: 0,
        }
    }
    
    pub fn record_operation(&mut self, node_id: &str) {
        *self.node_counts.entry(node_id.to_string()).or_insert(0) += 1;
        self.total_operations += 1;
    }
    
    /// 检查负载是否均衡（在指定的偏差范围内）
    pub fn is_balanced(&self, tolerance: f64) -> bool {
        if self.node_counts.is_empty() || self.total_operations == 0 {
            return true;
        }
        
        let expected_per_node = self.total_operations as f64 / self.node_counts.len() as f64;
        
        for count in self.node_counts.values() {
            let deviation = (*count as f64 - expected_per_node).abs() / expected_per_node;
            if deviation > tolerance {
                return false;
            }
        }
        
        true
    }
    
    pub fn get_distribution(&self) -> HashMap<String, f64> {
        if self.total_operations == 0 {
            return HashMap::new();
        }
        
        self.node_counts
            .iter()
            .map(|(node, count)| {
                (node.clone(), *count as f64 / self.total_operations as f64)
            })
            .collect()
    }
}

/// 数据一致性验证器
pub struct ConsistencyValidator {
    read_values: Vec<(String, String)>, // (key, value)
    write_operations: Vec<(String, String)>, // (key, value)
}

impl ConsistencyValidator {
    pub fn new() -> Self {
        Self {
            read_values: Vec::new(),
            write_operations: Vec::new(),
        }
    }
    
    pub fn record_write(&mut self, key: String, value: String) {
        self.write_operations.push((key, value));
    }
    
    pub fn record_read(&mut self, key: String, value: String) {
        self.read_values.push((key, value));
    }
    
    /// 检查最终一致性（所有读取都应该返回最新写入的值）
    pub fn check_eventual_consistency(&self) -> bool {
        let mut latest_writes: HashMap<String, String> = HashMap::new();
        
        // 构建最新写入值的映射
        for (key, value) in &self.write_operations {
            latest_writes.insert(key.clone(), value.clone());
        }
        
        // 检查所有读取是否返回最新值
        for (key, value) in &self.read_values {
            if let Some(expected_value) = latest_writes.get(key) {
                if value != expected_value {
                    return false;
                }
            }
        }
        
        true
    }
    
    /// 获取一致性统计
    pub fn get_consistency_stats(&self) -> ConsistencyStats {
        let mut correct_reads = 0;
        let mut stale_reads = 0;
        
        let mut latest_writes: HashMap<String, String> = HashMap::new();
        for (key, value) in &self.write_operations {
            latest_writes.insert(key.clone(), value.clone());
        }
        
        for (key, value) in &self.read_values {
            if let Some(expected_value) = latest_writes.get(key) {
                if value == expected_value {
                    correct_reads += 1;
                } else {
                    stale_reads += 1;
                }
            }
        }
        
        ConsistencyStats {
            total_reads: self.read_values.len(),
            correct_reads,
            stale_reads,
            consistency_ratio: if self.read_values.is_empty() {
                1.0
            } else {
                correct_reads as f64 / self.read_values.len() as f64
            },
        }
    }
}

/// 一致性统计信息
#[derive(Debug, Clone)]
pub struct ConsistencyStats {
    pub total_reads: usize,
    pub correct_reads: usize,
    pub stale_reads: usize,
    pub consistency_ratio: f64,
}

/// 测试数据集生成器
pub struct TestDataGenerator {
    dimension: usize,
    document_count: usize,
}

impl TestDataGenerator {
    pub fn new(dimension: usize, document_count: usize) -> Self {
        Self {
            dimension,
            document_count,
        }
    }
    
    /// 生成聚类数据集（多个聚类中心）
    pub fn generate_clustered_dataset(&self, cluster_count: usize) -> Vec<Document> {
        let mut documents = Vec::new();
        let docs_per_cluster = self.document_count / cluster_count;
        
        for cluster_id in 0..cluster_count {
            // 生成聚类中心
            let center: Vec<f32> = (0..self.dimension)
                .map(|_| fastrand::f32() * 2.0 - 1.0)
                .collect();
            
            // 在聚类中心周围生成文档
            for doc_id in 0..docs_per_cluster {
                let mut vector = center.clone();
                
                // 添加噪声
                for i in 0..self.dimension {
                    vector[i] += (fastrand::f32() - 0.5) * 0.2;
                }
                
                let document = Document {
                    id: format!("cluster_{}_doc_{}", cluster_id, doc_id),
                    title: Some(format!("聚类 {} 文档 {}", cluster_id, doc_id)),
                    content: format!("这是聚类{}中的第{}个文档", cluster_id, doc_id),
                    language: Some("zh".to_string()),
                    version: Some("1".to_string()),
                    doc_type: Some("test".to_string()),
                    package_name: Some("test_package".to_string()),
                    vector: Some(vector),
                    metadata: {
                        let mut meta = HashMap::new();
                        meta.insert("cluster_id".to_string(), cluster_id.to_string());
                        meta
                    },
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                
                documents.push(document);
            }
        }
        
        documents
    }
    
    /// 生成均匀分布的数据集
    pub fn generate_uniform_dataset(&self) -> Vec<Document> {
        (0..self.document_count)
            .map(|i| {
                let vector = generate_random_vector(self.dimension);
                create_test_document_with_vector(&format!("uniform_doc_{}", i), vector)
            })
            .collect()
    }
}

/// 断言宏扩展
#[macro_export]
macro_rules! assert_eventually {
    ($condition:expr, $timeout:expr) => {{
        let result = $crate::test_framework::utils::wait_for_condition(
            || async { $condition },
            $timeout,
            std::time::Duration::from_millis(100),
        ).await;
        assert!(result, "条件在超时时间内未满足");
    }};
}

#[macro_export]
macro_rules! assert_performance {
    ($throughput:expr, $min_qps:expr) => {{
        assert!(
            $throughput >= $min_qps,
            "性能不满足要求: 实际 {:.2} QPS < 期望 {:.2} QPS",
            $throughput,
            $min_qps
        );
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_test_vector() {
        let vector = generate_test_vector(1);
        assert_eq!(vector.len(), 128);
        assert_eq!(vector[0], 0.128);
        assert_eq!(vector[127], 0.255);
    }
    
    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!(approximately_equal(cosine_similarity(&a, &b), 1.0, 0.001));
        
        let c = vec![0.0, 1.0, 0.0];
        assert!(approximately_equal(cosine_similarity(&a, &c), 0.0, 0.001));
    }
    
    #[test]
    fn test_performance_tester() {
        let mut tester = PerformanceTester::new();
        
        // 模拟一些操作
        for _ in 0..100 {
            tester.record_operation();
        }
        
        let throughput = tester.get_throughput();
        assert!(throughput > 0.0);
    }
    
    #[test]
    fn test_load_balance_validator() {
        let mut validator = LoadBalanceValidator::new();
        
        // 均匀分布的操作
        for i in 0..300 {
            validator.record_operation(&format!("node_{}", i % 3));
        }
        
        assert!(validator.is_balanced(0.1)); // 10%的容差
        
        let distribution = validator.get_distribution();
        assert_eq!(distribution.len(), 3);
        
        for percentage in distribution.values() {
            assert!((*percentage - 1.0/3.0).abs() < 0.1);
        }
    }
    
    #[test]
    fn test_test_data_generator() {
        let generator = TestDataGenerator::new(128, 100);
        let dataset = generator.generate_clustered_dataset(5);
        
        assert_eq!(dataset.len(), 100);
        
        for doc in &dataset {
            assert_eq!(doc.vector.as_ref().unwrap().len(), 128);
            assert!(doc.metadata.contains_key("cluster_id"));
        }
    }
    
    #[tokio::test]
    async fn test_wait_for_condition() {
        let mut counter = 0;
        
        let result = wait_for_condition(
            || {
                counter += 1;
                async move { counter >= 5 }
            },
            std::time::Duration::from_secs(1),
            std::time::Duration::from_millis(10),
        ).await;
        
        assert!(result);
        assert!(counter >= 5);
    }
}