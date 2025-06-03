// 索引模块 - 将来用于HNSW索引实现

use crate::{types::VectorPoint, config::HnswConfig, errors::{Result, VectorDbError}};
use instant_distance::{Builder, Hnsw, Point};
use std::collections::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// 向量点的包装，实现Point trait
#[derive(Debug, Clone)]
pub struct HnswPoint {
    pub vector: Vec<f32>,
    pub document_id: String,
}

impl Point for HnswPoint {
    fn distance(&self, other: &Self) -> f32 {
        // 计算欧几里得距离
        self.vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

/// HNSW索引实现
pub struct HnswIndex {
    index: Arc<RwLock<Option<Hnsw<HnswPoint>>>>,
    points: Arc<RwLock<Vec<HnswPoint>>>,
    id_to_index: Arc<RwLock<HashMap<String, usize>>>,
    config: HnswConfig,
    dimension: usize,
}

impl HnswIndex {
    pub fn new(config: HnswConfig, dimension: usize) -> Self {
        Self {
            index: Arc::new(RwLock::new(None)),
            points: Arc::new(RwLock::new(Vec::new())),
            id_to_index: Arc::new(RwLock::new(HashMap::new())),
            config,
            dimension,
        }
    }

    /// 添加向量点到索引
    pub fn add_point(&self, point: VectorPoint) -> Result<()> {
        if point.vector.len() != self.dimension {
            return Err(VectorDbError::InvalidVectorDimension {
                expected: self.dimension,
                actual: point.vector.len(),
            });
        }

        let hnsw_point = HnswPoint {
            vector: point.vector,
            document_id: point.document_id.clone(),
        };

        let mut points = self.points.write();
        let mut id_to_index = self.id_to_index.write();
        
        // 检查是否已存在
        if id_to_index.contains_key(&point.document_id) {
            // 更新现有点
            let index = id_to_index[&point.document_id];
            points[index] = hnsw_point;
        } else {
            // 添加新点
            let index = points.len();
            points.push(hnsw_point);
            id_to_index.insert(point.document_id, index);
        }

        // 标记索引需要重建
        *self.index.write() = None;

        Ok(())
    }

    /// 删除向量点
    pub fn remove_point(&self, document_id: &str) -> Result<bool> {
        let mut points = self.points.write();
        let mut id_to_index = self.id_to_index.write();

        if let Some(&index) = id_to_index.get(document_id) {
            // 移除点（通过交换到末尾然后pop）
            let last_index = points.len() - 1;
            if index != last_index {
                points.swap(index, last_index);
                // 更新被交换点的索引映射
                let swapped_id = &points[index].document_id;
                id_to_index.insert(swapped_id.clone(), index);
            }
            points.pop();
            id_to_index.remove(document_id);

            // 标记索引需要重建
            *self.index.write() = None;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 构建或重建索引
    pub fn build_index(&self) -> Result<()> {
        let points = self.points.read();
        
        if points.is_empty() {
            return Ok(());
        }

        let builder = Builder::default()
            .m(self.config.m)
            .ef_construction(self.config.ef_construction)
            .ml(1.0 / (2.0_f32).ln())
            .seed(42);

        let hnsw = builder.build(points.clone())
            .map_err(|e| VectorDbError::index_error(format!("构建HNSW索引失败: {}", e)))?;

        *self.index.write() = Some(hnsw);

        Ok(())
    }

    /// 搜索最相似的向量
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        if query.len() != self.dimension {
            return Err(VectorDbError::InvalidVectorDimension {
                expected: self.dimension,
                actual: query.len(),
            });
        }

        // 确保索引已构建
        if self.index.read().is_none() {
            drop(self.index.read()); // 释放读锁
            self.build_index()?;
        }

        let index_guard = self.index.read();
        let index = index_guard.as_ref().ok_or_else(|| {
            VectorDbError::index_error("索引未构建".to_string())
        })?;

        let query_point = HnswPoint {
            vector: query.to_vec(),
            document_id: "query".to_string(),
        };

        let search_results = index.search(&query_point, k);
        
        let mut results = Vec::new();
        for item in search_results {
            let point = &item.item;
            results.push(SearchResult {
                document_id: point.document_id.clone(),
                distance: item.distance,
                similarity: 1.0 / (1.0 + item.distance), // 转换为相似度分数
            });
        }

        Ok(results)
    }

    /// 获取索引统计信息
    pub fn stats(&self) -> IndexStats {
        let points = self.points.read();
        let has_index = self.index.read().is_some();
        
        IndexStats {
            point_count: points.len(),
            dimension: self.dimension,
            is_built: has_index,
            memory_usage_mb: self.estimate_memory_usage(),
        }
    }

    /// 估算内存使用量
    fn estimate_memory_usage(&self) -> f64 {
        let points = self.points.read();
        let point_size = self.dimension * 4 + 64; // 向量 + 元数据
        let total_bytes = points.len() * point_size;
        
        // HNSW索引的额外开销（估算）
        let index_overhead = if self.index.read().is_some() {
            total_bytes * 2 // 大致估算
        } else {
            0
        };
        
        (total_bytes + index_overhead) as f64 / (1024.0 * 1024.0)
    }

    /// 保存索引到文件
    pub async fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        let points = self.points.read().clone();
        let id_to_index = self.id_to_index.read().clone();
        
        let index_data = IndexData {
            points: points.into_iter().map(|p| SerializablePoint {
                vector: p.vector,
                document_id: p.document_id,
            }).collect(),
            id_to_index,
            config: self.config.clone(),
            dimension: self.dimension,
        };

        let data = bincode::serialize(&index_data)
            .map_err(|e| VectorDbError::index_error(format!("序列化索引失败: {}", e)))?;

        tokio::fs::write(path, data).await
            .map_err(|e| VectorDbError::index_error(format!("保存索引文件失败: {}", e)))?;

        Ok(())
    }

    /// 从文件加载索引
    pub async fn load_from_file(&self, path: &std::path::Path) -> Result<()> {
        let data = tokio::fs::read(path).await
            .map_err(|e| VectorDbError::index_error(format!("读取索引文件失败: {}", e)))?;

        let index_data: IndexData = bincode::deserialize(&data)
            .map_err(|e| VectorDbError::index_error(format!("反序列化索引失败: {}", e)))?;

        // 验证维度匹配
        if index_data.dimension != self.dimension {
            return Err(VectorDbError::InvalidVectorDimension {
                expected: self.dimension,
                actual: index_data.dimension,
            });
        }

        // 恢复数据
        let hnsw_points: Vec<HnswPoint> = index_data.points.into_iter().map(|p| HnswPoint {
            vector: p.vector,
            document_id: p.document_id,
        }).collect();

        *self.points.write() = hnsw_points;
        *self.id_to_index.write() = index_data.id_to_index;

        // 重建索引
        self.build_index()?;

        Ok(())
    }
}

/// 搜索结果
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub document_id: String,
    pub distance: f32,
    pub similarity: f32,
}

/// 索引统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub point_count: usize,
    pub dimension: usize,
    pub is_built: bool,
    pub memory_usage_mb: f64,
}

/// 可序列化的点数据
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializablePoint {
    vector: Vec<f32>,
    document_id: String,
}

/// 索引数据（用于序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexData {
    points: Vec<SerializablePoint>,
    id_to_index: HashMap<String, usize>,
    config: HnswConfig,
    dimension: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hnsw_index_basic() {
        let config = HnswConfig::default();
        let index = HnswIndex::new(config, 3);

        // 添加一些测试点
        let point1 = VectorPoint {
            vector: vec![1.0, 0.0, 0.0],
            document_id: "doc1".to_string(),
        };
        let point2 = VectorPoint {
            vector: vec![0.0, 1.0, 0.0],
            document_id: "doc2".to_string(),
        };

        assert!(index.add_point(point1).is_ok());
        assert!(index.add_point(point2).is_ok());

        // 构建索引
        assert!(index.build_index().is_ok());

        // 搜索
        let query = vec![1.0, 0.1, 0.0];
        let results = index.search(&query, 2).unwrap();
        
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].document_id, "doc1"); // 应该最相似
    }

    #[test]
    fn test_dimension_validation() {
        let config = HnswConfig::default();
        let index = HnswIndex::new(config, 3);

        let wrong_point = VectorPoint {
            vector: vec![1.0, 0.0], // 错误的维度
            document_id: "doc1".to_string(),
        };

        assert!(index.add_point(wrong_point).is_err());
    }
} 