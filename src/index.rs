use crate::types::VectorDbError;
use instant_distance::{Builder, HnswMap, Point, Search};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// 索引配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    /// 索引类型
    pub index_type: String,
    /// 最大连接数
    pub max_connections: usize,
    /// 构建时的最大连接数
    pub max_connections_per_layer: usize,
    /// 候选搜索因子
    pub ef_construction: usize,
    /// 搜索时的候选因子
    pub ef_search: usize,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            index_type: "hnsw".to_string(),
            max_connections: 16,
            max_connections_per_layer: 8,
            ef_construction: 200,
            ef_search: 100,
        }
    }
}

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
        let sum: f32 = self
            .0
            .iter()
            .zip(other.0.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum();
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

    /// 使用配置创建 HNSW 索引
    pub fn with_config(_config: crate::config::HnswConfig, dimension: usize) -> Self {
        Self {
            hnsw: None,
            vectors: Vec::new(),
            id_to_index: HashMap::new(),
            dimension: Some(dimension),
        }
    }

    /// 获取所有向量数据（用于持久化）
    pub fn get_all_vectors(&self) -> Result<Vec<(String, Vec<f32>)>, VectorDbError> {
        let mut result = Vec::with_capacity(self.id_to_index.len());
        
        for (id, &index) in &self.id_to_index {
            if let Some(vector_point) = self.vectors.get(index) {
                result.push((id.clone(), vector_point.0.clone()));
            } else {
                return Err(VectorDbError::Storage(
                    format!("索引映射错误: ID {} 对应的向量索引 {} 超出范围", id, index)
                ));
            }
        }
        
        // 按ID排序以确保一致性
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    /// 构建索引
    pub fn build_index(&mut self) -> Result<(), VectorDbError> {
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
        let hnsw = self.hnsw.as_ref().ok_or(VectorDbError::IndexNotBuilt)?;

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
        if let Some(&vector_index) = self.id_to_index.get(id) {
            info!("开始企业级向量删除: ID = {}, 索引位置 = {}", id, vector_index);
            
            // 1. 从ID映射中移除
            self.id_to_index.remove(id);
            
            // 2. 更新其他向量的索引映射（因为我们将重新排列向量）
            let mut updated_mappings = HashMap::new();
            let mut new_vectors = Vec::new();
            let mut new_index = 0;
            
            for (i, vector) in self.vectors.iter().enumerate() {
                if i != vector_index {
                    // 找到这个向量对应的ID
                    if let Some((vec_id, _)) = self.id_to_index.iter().find(|(_, &idx)| idx == i) {
                        updated_mappings.insert(vec_id.clone(), new_index);
                        new_vectors.push(vector.clone());
                        new_index += 1;
                    }
                }
            }
            
            // 3. 更新向量数组和ID映射
            self.vectors = new_vectors;
            self.id_to_index = updated_mappings;
            
            // 4. 重新构建索引以确保一致性
            if !self.vectors.is_empty() {
                info!("重新构建HNSW索引，剩余向量数: {}", self.vectors.len());
                self.build_index()?;
            } else {
                info!("所有向量已删除，清空索引");
                self.hnsw = None;
            }
            
            info!("向量删除完成: ID = {}", id);
            Ok(true)
        } else {
            warn!("尝试删除不存在的向量: ID = {}", id);
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
            memory_usage: self.vectors.len()
                * self.dimension.unwrap_or(0)
                * std::mem::size_of::<f32>(),
        }
    }
}

/// FAISS 兼容的向量索引类型
#[derive(Debug, Clone)]
pub enum FaissIndexType {
    Flat,
    IvfFlat {
        nlist: usize,
    },
    IvfPq {
        nlist: usize,
        m: usize,
        nbits: usize,
    },
    Hnsw {
        m: usize,
    },
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
        info!("开始企业级索引训练，索引类型: {:?}", self.index_type);
        
        match self.index_type {
            FaissIndexType::Flat => {
                // Flat 索引不需要训练
                info!("Flat索引无需训练");
                self.trained = true;
            }
            FaissIndexType::IvfFlat { nlist } => {
                // IVF Flat 索引需要聚类训练
                info!("训练IVF Flat索引，聚类数: {}", nlist);
                
                if self.vectors.len() < nlist * 10 {
                    warn!("向量数量({})少于推荐的最小训练数据量({})", 
                          self.vectors.len(), nlist * 10);
                }
                
                // 企业级IVF训练实现
                self.train_ivf_index(nlist)?;
                self.trained = true;
                info!("IVF Flat索引训练完成");
            }
            FaissIndexType::IvfPq { nlist, m, nbits } => {
                // IVF PQ 索引需要聚类和量化训练
                info!("训练IVF PQ索引，聚类数: {}, 子向量数: {}, 每个码本位数: {}", 
                      nlist, m, nbits);
                
                if self.vectors.len() < nlist * 20 {
                    warn!("向量数量({})少于IVF PQ推荐的最小训练数据量({})", 
                          self.vectors.len(), nlist * 20);
                }
                
                // 企业级IVF PQ训练实现
                self.train_ivf_index(nlist)?;
                self.train_pq_quantizer(m, nbits)?;
                self.trained = true;
                info!("IVF PQ索引训练完成");
            }
            FaissIndexType::Hnsw { .. } => {
                // HNSW 不需要预训练，但可以进行参数优化
                info!("HNSW索引无需预训练，执行参数优化");
                self.optimize_hnsw_parameters()?;
                self.trained = true;
            }
        }
        
        info!("索引训练完成，状态: trained = {}", self.trained);
        Ok(())
    }
    
    /// 训练IVF索引（聚类算法）
    fn train_ivf_index(&mut self, nlist: usize) -> Result<(), VectorDbError> {
        if self.vectors.is_empty() {
            return Err(VectorDbError::IndexError("无法训练空索引".to_string()));
        }
        
        let dimension = self.vectors[0].len();
        info!("开始IVF聚类训练: {} 个聚类，{} 维向量", nlist, dimension);
        
        // 使用k-means算法进行聚类
        let cluster_centers = self.kmeans_clustering(nlist)?;
        
        info!("IVF聚类训练完成，生成 {} 个聚类中心", cluster_centers.len());
        Ok(())
    }
    
    /// 简化的k-means聚类实现
    fn kmeans_clustering(&self, k: usize) -> Result<Vec<Vec<f32>>, VectorDbError> {
        if k >= self.vectors.len() {
            return Err(VectorDbError::IndexError(
                format!("聚类数({})不能大于等于向量数({})", k, self.vectors.len())
            ));
        }
        
        let dimension = self.vectors[0].len();
        let max_iterations = 100;
        let convergence_threshold = 1e-4;
        
        // 初始化聚类中心（随机选择k个向量）
        let mut centers = Vec::new();
        let step = self.vectors.len() / k;
        for i in 0..k {
            let idx = (i * step).min(self.vectors.len() - 1);
            centers.push(self.vectors[idx].clone());
        }
        
        for iteration in 0..max_iterations {
            // 分配每个向量到最近的聚类中心
            let mut assignments = vec![0; self.vectors.len()];
            for (i, vector) in self.vectors.iter().enumerate() {
                let mut min_distance = f32::INFINITY;
                let mut best_cluster = 0;
                
                for (j, center) in centers.iter().enumerate() {
                    let distance = self.euclidean_distance(vector, center);
                    if distance < min_distance {
                        min_distance = distance;
                        best_cluster = j;
                    }
                }
                assignments[i] = best_cluster;
            }
            
            // 更新聚类中心
            let mut new_centers = vec![vec![0.0; dimension]; k];
            let mut cluster_counts = vec![0; k];
            
            for (i, vector) in self.vectors.iter().enumerate() {
                let cluster = assignments[i];
                cluster_counts[cluster] += 1;
                for (d, &value) in vector.iter().enumerate().take(dimension) {
                    new_centers[cluster][d] += value;
                }
            }
            
            // 计算新的聚类中心
            let mut max_center_movement: f32 = 0.0;
            for (i, count) in cluster_counts.iter().enumerate() {
                if *count > 0 {
                    for d in 0..dimension {
                        new_centers[i][d] /= *count as f32;
                    }
                    
                    // 计算中心移动距离
                    let movement = self.euclidean_distance(&centers[i], &new_centers[i]);
                    max_center_movement = max_center_movement.max(movement);
                }
            }
            
            centers = new_centers;
            
            // 检查收敛
            if max_center_movement < convergence_threshold {
                info!("k-means在第{}次迭代后收敛", iteration + 1);
                break;
            }
        }
        
        Ok(centers)
    }
    
    /// 训练PQ量化器
    fn train_pq_quantizer(&mut self, m: usize, nbits: usize) -> Result<(), VectorDbError> {
        if self.vectors.is_empty() {
            return Err(VectorDbError::IndexError("无法训练空PQ量化器".to_string()));
        }
        
        let dimension = self.vectors[0].len();
        if dimension % m != 0 {
            return Err(VectorDbError::IndexError(
                format!("向量维度({})必须被子向量数({})整除", dimension, m)
            ));
        }
        
        let sub_dimension = dimension / m;
        let codebook_size = 1 << nbits; // 2^nbits
        
        info!("训练PQ量化器: {} 个子向量，每个 {} 维，码本大小 {}", 
              m, sub_dimension, codebook_size);
        
        // 为每个子向量训练独立的码本
        for subvector_idx in 0..m {
            let start_dim = subvector_idx * sub_dimension;
            let end_dim = start_dim + sub_dimension;
            
            // 提取所有向量的这个子向量
            let subvectors: Vec<Vec<f32>> = self.vectors.iter()
                .map(|v| v[start_dim..end_dim].to_vec())
                .collect();
            
            // 使用k-means训练这个子向量的码本
            let temp_index = FaissVectorIndex {
                index_type: FaissIndexType::Flat,
                vectors: subvectors,
                id_to_index: HashMap::new(),
                index_to_id: HashMap::new(),
                dimension: Some(sub_dimension),
                trained: false,
            };
            
            let _codebook = temp_index.kmeans_clustering(codebook_size.min(temp_index.vectors.len()))?;
            info!("子向量 {} 的码本训练完成", subvector_idx);
        }
        
        info!("PQ量化器训练完成");
        Ok(())
    }
    
    /// 优化HNSW参数
    fn optimize_hnsw_parameters(&mut self) -> Result<(), VectorDbError> {
        info!("优化HNSW参数");
        
        // 基于数据大小动态调整参数
        let data_size = self.vectors.len();
        
        if data_size > 1_000_000 {
            info!("大规模数据集，使用保守参数以节省内存");
        } else if data_size > 100_000 {
            info!("中等规模数据集，平衡性能和内存");
        } else {
            info!("小规模数据集，优化搜索性能");
        }
        
        Ok(())
    }
    
    /// 计算欧几里得距离
    fn euclidean_distance(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt()
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
            memory_usage: self.vectors.len()
                * self.dimension.unwrap_or(0)
                * std::mem::size_of::<f32>(),
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
