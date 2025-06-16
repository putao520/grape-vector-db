use crate::types::VectorDbError;
use instant_distance::{Builder, Search, HnswMap, Point};
use std::collections::HashMap;

/// 向量索引特征
pub trait VectorIndex: Send + Sync {
    /// 添加向量到索引
    fn add_vector(&mut self, id: String, vector: Vec<f32>) -> Result<(), VectorDbError>;
    
    /// 批量添加向量
    fn add_vectors(&mut self, vectors: Vec<(String, Vec<f32>)>) -> Result<(), VectorDbError>;
    
    /// 搜索最相似的向量
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>, VectorDbError>;
    
    /// 删除向量
    fn remove_vector(&mut self, id: &str) -> Result<bool, VectorDbError>;
    
    /// 获取向量数量
    fn len(&self) -> usize;
    
    /// 检查是否为空
    fn is_empty(&self) -> bool;
    
    /// 优化索引
    fn optimize(&mut self) -> Result<(), VectorDbError>;
    
    /// 清空索引
    fn clear(&mut self);
    
    /// 获取索引统计信息
    fn get_stats(&self) -> IndexStats;
}

/// 向量点，实现 instant_distance::Point trait
#[derive(Debug, Clone)]
pub struct VectorPoint(pub Vec<f32>);

impl Point for VectorPoint {
    fn distance(&self, other: &Self) -> f32 {
        // 欧几里得距离
        let sum: f32 = self.0.iter().zip(other.0.iter()).map(|(x, y)| (x - y).powi(2)).sum();
        sum.sqrt()
    }
}

/// 索引统计信息
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub vector_count: usize,
    pub dimension: usize,
    pub index_type: String,
    pub memory_usage: usize,
}

/// HNSW 向量索引实现
pub struct HnswVectorIndex {
    hnsw: Option<HnswMap<VectorPoint, String>>,
    vectors: Vec<VectorPoint>,
    id_to_index: HashMap<String, usize>,
    dimension: Option<usize>,
}

impl HnswVectorIndex {
    /// 创建新的 HNSW 索引
    pub fn new() -> Self {
        Self {
            hnsw: None,
            vectors: Vec::new(),
            id_to_index: HashMap::new(),
            dimension: None,
        }
    }
    
    /// 构建索引
    fn build_index(&mut self) -> Result<(), VectorDbError> {
        if self.vectors.is_empty() {
            return Ok(());
        }
        
        // 创建 ID 列表
        let ids: Vec<String> = (0..self.vectors.len())
            .map(|i| format!("vec_{}", i))
            .collect();
        
        let hnsw = Builder::default().build(self.vectors.clone(), ids);
        self.hnsw = Some(hnsw);
        
        Ok(())
    }
}

impl Default for HnswVectorIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorIndex for HnswVectorIndex {
    fn add_vector(&mut self, id: String, vector: Vec<f32>) -> Result<(), VectorDbError> {
        // 检查维度一致性
        if let Some(dim) = self.dimension {
            if vector.len() != dim {
                return Err(VectorDbError::DimensionMismatch {
                    expected: dim,
                    actual: vector.len(),
                });
            }
        } else {
            self.dimension = Some(vector.len());
        }
        
        let index = self.vectors.len();
        self.vectors.push(VectorPoint(vector));
        self.id_to_index.insert(id, index);
        
        // 重新构建索引
        self.build_index()?;
        
        Ok(())
    }
    
    fn add_vectors(&mut self, vectors: Vec<(String, Vec<f32>)>) -> Result<(), VectorDbError> {
        for (id, vector) in vectors {
            // 检查维度一致性
            if let Some(dim) = self.dimension {
                if vector.len() != dim {
                    return Err(VectorDbError::DimensionMismatch {
                        expected: dim,
                        actual: vector.len(),
                    });
                }
            } else {
                self.dimension = Some(vector.len());
            }
            
            let index = self.vectors.len();
            self.vectors.push(VectorPoint(vector));
            self.id_to_index.insert(id, index);
        }
        
        // 批量添加后重新构建索引
        self.build_index()?;
        
        Ok(())
    }
    
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>, VectorDbError> {
        let hnsw = self.hnsw.as_ref()
            .ok_or_else(|| VectorDbError::IndexNotBuilt)?;
        
        let query_point = VectorPoint(query.to_vec());
        let mut search = Search::default();
        let results: Vec<_> = hnsw.search(&query_point, &mut search).take(k).collect();
        
        let mut final_results = Vec::new();
        for item in results {
            // 从 id_to_index 中找到对应的 ID
            for (id, &index) in &self.id_to_index {
                if format!("vec_{}", index) == *item.value {
                    final_results.push((id.clone(), item.distance));
                    break;
                }
            }
        }
        
        Ok(final_results)
    }
    
    fn remove_vector(&mut self, id: &str) -> Result<bool, VectorDbError> {
        if self.id_to_index.contains_key(id) {
            // 简单实现：标记为删除，实际删除需要重建索引
            self.id_to_index.remove(id);
            
            // 重新构建索引以移除向量
            self.build_index()?;
            
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    fn len(&self) -> usize {
        self.id_to_index.len()
    }
    
    fn is_empty(&self) -> bool {
        self.id_to_index.is_empty()
    }
    
    fn optimize(&mut self) -> Result<(), VectorDbError> {
        // 重新构建索引进行优化
        self.build_index()
    }
    
    fn clear(&mut self) {
        self.hnsw = None;
        self.vectors.clear();
        self.id_to_index.clear();
        self.dimension = None;
    }
    
    fn get_stats(&self) -> IndexStats {
        IndexStats {
            vector_count: self.len(),
            dimension: self.dimension.unwrap_or(0),
            index_type: "HNSW".to_string(),
            memory_usage: self.vectors.len() * self.dimension.unwrap_or(0) * std::mem::size_of::<f32>(),
        }
    }
}

/// FAISS 兼容的向量索引类型
#[derive(Debug, Clone)]
pub enum FaissIndexType {
    Flat,
    IvfFlat { nlist: usize },
    IvfPq { nlist: usize, m: usize, nbits: usize },
    Hnsw { m: usize },
}

/// FAISS 兼容的向量索引实现
pub struct FaissVectorIndex {
    index_type: FaissIndexType,
    vectors: Vec<Vec<f32>>,
    id_to_index: HashMap<String, usize>,
    index_to_id: HashMap<usize, String>,
    dimension: Option<usize>,
    trained: bool,
}

impl FaissVectorIndex {
    /// 创建新的 FAISS 兼容索引
    pub fn new(index_type: FaissIndexType) -> Self {
        Self {
            index_type,
            vectors: Vec::new(),
            id_to_index: HashMap::new(),
            index_to_id: HashMap::new(),
            dimension: None,
            trained: false,
        }
    }
    
    /// 训练索引（对于需要训练的索引类型）
    pub fn train(&mut self) -> Result<(), VectorDbError> {
        match self.index_type {
            FaissIndexType::Flat => {
                // Flat 索引不需要训练
                self.trained = true;
            }
            FaissIndexType::IvfFlat { .. } | 
            FaissIndexType::IvfPq { .. } => {
                // 简化实现：标记为已训练
                self.trained = true;
            }
            FaissIndexType::Hnsw { .. } => {
                // HNSW 不需要预训练
                self.trained = true;
            }
        }
        Ok(())
    }
}

impl VectorIndex for FaissVectorIndex {
    fn add_vector(&mut self, id: String, vector: Vec<f32>) -> Result<(), VectorDbError> {
        // 检查维度一致性
        if let Some(dim) = self.dimension {
            if vector.len() != dim {
                return Err(VectorDbError::DimensionMismatch {
                    expected: dim,
                    actual: vector.len(),
                });
            }
        } else {
            self.dimension = Some(vector.len());
        }
        
        // 确保索引已训练
        if !self.trained {
            self.train()?;
        }
        
        let index = self.vectors.len();
        self.vectors.push(vector);
        self.id_to_index.insert(id.clone(), index);
        self.index_to_id.insert(index, id);
        
        Ok(())
    }
    
    fn add_vectors(&mut self, vectors: Vec<(String, Vec<f32>)>) -> Result<(), VectorDbError> {
        for (id, vector) in vectors {
            self.add_vector(id, vector)?;
        }
        Ok(())
    }
    
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>, VectorDbError> {
        if !self.trained {
            return Err(VectorDbError::IndexNotBuilt);
        }
        
        let mut results = Vec::new();
        
        // 简单的线性搜索实现
        for (index, vector) in self.vectors.iter().enumerate() {
            if let Some(id) = self.index_to_id.get(&index) {
                let distance = cosine_distance(query, vector);
                results.push((id.clone(), distance));
            }
        }
        
        // 按距离排序并取前k个
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        
        Ok(results)
    }
    
    fn remove_vector(&mut self, id: &str) -> Result<bool, VectorDbError> {
        if let Some(&index) = self.id_to_index.get(id) {
            self.id_to_index.remove(id);
            self.index_to_id.remove(&index);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    fn len(&self) -> usize {
        self.id_to_index.len()
    }
    
    fn is_empty(&self) -> bool {
        self.id_to_index.is_empty()
    }
    
    fn optimize(&mut self) -> Result<(), VectorDbError> {
        // 重新训练索引
        self.train()
    }
    
    fn clear(&mut self) {
        self.vectors.clear();
        self.id_to_index.clear();
        self.index_to_id.clear();
        self.dimension = None;
        self.trained = false;
    }
    
    fn get_stats(&self) -> IndexStats {
        IndexStats {
            vector_count: self.len(),
            dimension: self.dimension.unwrap_or(0),
            index_type: format!("{:?}", self.index_type),
            memory_usage: self.vectors.len() * self.dimension.unwrap_or(0) * std::mem::size_of::<f32>(),
        }
    }
}

/// 计算余弦距离
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }
    
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return f32::INFINITY;
    }
    
    1.0 - (dot_product / (norm_a * norm_b))
}

/// 索引优化器
pub struct IndexOptimizer {
    optimization_strategies: Vec<OptimizationStrategy>,
}

/// 优化策略
#[derive(Debug, Clone)]
pub enum OptimizationStrategy {
    /// 定期重建索引
    PeriodicRebuild { interval_seconds: u64 },
    /// 基于向量数量的优化
    VectorCountBased { threshold: usize },
    /// 基于内存使用的优化
    MemoryBased { threshold_mb: usize },
}

impl IndexOptimizer {
    /// 创建新的索引优化器
    pub fn new() -> Self {
        Self {
            optimization_strategies: vec![
                OptimizationStrategy::VectorCountBased { threshold: 10000 },
                OptimizationStrategy::MemoryBased { threshold_mb: 100 },
            ],
        }
    }
    
    /// 添加优化策略
    pub fn add_strategy(&mut self, strategy: OptimizationStrategy) {
        self.optimization_strategies.push(strategy);
    }
    
    /// 检查是否需要优化
    pub fn should_optimize(&self, stats: &IndexStats) -> bool {
        for strategy in &self.optimization_strategies {
            match strategy {
                OptimizationStrategy::VectorCountBased { threshold } => {
                    if stats.vector_count >= *threshold {
                        return true;
                    }
                }
                OptimizationStrategy::MemoryBased { threshold_mb } => {
                    let memory_mb = stats.memory_usage / (1024 * 1024);
                    if memory_mb >= *threshold_mb {
                        return true;
                    }
                }
                OptimizationStrategy::PeriodicRebuild { .. } => {
                    // 需要额外的时间跟踪逻辑
                    // 这里简化处理
                    continue;
                }
            }
        }
        false
    }
    
    /// 执行优化
    pub fn optimize(&self, index: &mut dyn VectorIndex) -> Result<(), VectorDbError> {
        index.optimize()
    }
}

impl Default for IndexOptimizer {
    fn default() -> Self {
        Self::new()
    }
} 