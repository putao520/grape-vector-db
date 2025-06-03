use crate::{types::*, config::VectorDbConfig, errors::{Result, VectorDbError}};
use std::path::PathBuf;
use async_trait::async_trait;
use sled::{Db, Tree};
use bincode;
use tokio::task;
use std::sync::Arc;
use parking_lot::RwLock;
use moka::future::Cache;
use std::time::Duration;

/// 向量存储trait
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn add_document(&mut self, record: DocumentRecord) -> Result<()>;
    async fn get_document(&self, id: &str) -> Result<Option<DocumentRecord>>;
    async fn search_similar(&self, query_vector: &[f32], limit: usize) -> Result<Vec<String>>;
    async fn delete_document(&mut self, id: &str) -> Result<bool>;
    async fn update_document(&mut self, record: DocumentRecord) -> Result<()>;
    async fn list_documents(&self, offset: usize, limit: usize) -> Result<Vec<DocumentRecord>>;
    async fn save(&self) -> Result<()>;
    async fn load(&mut self) -> Result<()>;
    async fn compact(&self) -> Result<()>;
    fn stats(&self) -> DatabaseStats;
}

/// 基于Sled的向量存储实现
pub struct SledVectorStore {
    db: Arc<Db>,
    documents: Tree,
    vectors: Tree,
    metadata: Tree,
    cache: Cache<String, DocumentRecord>,
    stats: Arc<RwLock<DatabaseStats>>,
    config: VectorDbConfig,
}

impl SledVectorStore {
    pub async fn new(data_dir: PathBuf, config: &VectorDbConfig) -> Result<Self> {
        // 创建数据目录
        tokio::fs::create_dir_all(&data_dir).await
            .map_err(|e| VectorDbError::storage_error(format!("无法创建数据目录: {}", e)))?;

        // 打开Sled数据库
        let db_path = data_dir.join("vector_db");
        let db = task::spawn_blocking(move || {
            sled::open(db_path)
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?
            .map_err(|e| VectorDbError::storage_error(format!("无法打开数据库: {}", e)))?;

        let db = Arc::new(db);

        // 打开不同的树（表）
        let documents = db.open_tree("documents")
            .map_err(|e| VectorDbError::storage_error(format!("无法打开文档树: {}", e)))?;
        let vectors = db.open_tree("vectors")
            .map_err(|e| VectorDbError::storage_error(format!("无法打开向量树: {}", e)))?;
        let metadata = db.open_tree("metadata")
            .map_err(|e| VectorDbError::storage_error(format!("无法打开元数据树: {}", e)))?;

        // 创建缓存
        let cache = Cache::builder()
            .max_capacity(config.cache.embedding_cache_size as u64)
            .time_to_live(Duration::from_secs(config.cache.cache_ttl_seconds))
            .build();

        // 初始化统计信息
        let stats = Arc::new(RwLock::new(DatabaseStats::default()));

        let mut store = Self {
            db,
            documents,
            vectors,
            metadata,
            cache,
            stats,
            config: config.clone(),
        };

        // 加载现有统计信息
        store.load_stats().await?;

        Ok(store)
    }

    async fn load_stats(&mut self) -> Result<()> {
        let metadata = self.metadata.clone();
        let stats = task::spawn_blocking(move || {
            if let Ok(Some(data)) = metadata.get("stats") {
                bincode::deserialize::<DatabaseStats>(&data)
                    .map_err(|e| VectorDbError::storage_error(format!("无法反序列化统计信息: {}", e)))
            } else {
                Ok(DatabaseStats::default())
            }
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))??;

        *self.stats.write() = stats;
        Ok(())
    }

    async fn save_stats(&self) -> Result<()> {
        let stats = self.stats.read().clone();
        let metadata = self.metadata.clone();
        
        task::spawn_blocking(move || {
            let data = bincode::serialize(&stats)
                .map_err(|e| VectorDbError::storage_error(format!("无法序列化统计信息: {}", e)))?;
            metadata.insert("stats", data)
                .map_err(|e| VectorDbError::storage_error(format!("无法保存统计信息: {}", e)))?;
            Ok::<(), VectorDbError>(())
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))??;

        Ok(())
    }

    fn update_stats(&self, delta_docs: i64, delta_vectors: i64) {
        let mut stats = self.stats.write();
        stats.document_count = (stats.document_count as i64 + delta_docs).max(0) as usize;
        stats.vector_count = (stats.vector_count as i64 + delta_vectors).max(0) as usize;
        
        // 估算内存使用量（简化计算）
        stats.memory_usage_mb = (stats.document_count * 1024 + stats.vector_count * self.config.vector_dimension * 4) as f64 / (1024.0 * 1024.0);
    }
}

#[async_trait]
impl VectorStore for SledVectorStore {
    async fn add_document(&mut self, record: DocumentRecord) -> Result<()> {
        let id = record.id.clone();
        let vector = record.embedding.clone();
        
        // 序列化文档记录
        let doc_data = bincode::serialize(&record)
            .map_err(|e| VectorDbError::storage_error(format!("无法序列化文档: {}", e)))?;
        
        // 序列化向量
        let vector_data = bincode::serialize(&vector)
            .map_err(|e| VectorDbError::storage_error(format!("无法序列化向量: {}", e)))?;

        let documents = self.documents.clone();
        let vectors = self.vectors.clone();
        let id_clone = id.clone();
        
        // 异步保存到数据库
        let was_new = task::spawn_blocking(move || {
            let was_new = !documents.contains_key(&id_clone)?;
            documents.insert(&id_clone, doc_data)?;
            vectors.insert(&id_clone, vector_data)?;
            Ok::<bool, sled::Error>(was_new)
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?
            .map_err(|e| VectorDbError::storage_error(format!("数据库操作失败: {}", e)))?;

        // 更新缓存
        self.cache.insert(id.clone(), record).await;

        // 更新统计信息
        if was_new {
            self.update_stats(1, 1);
        }

        Ok(())
    }

    async fn get_document(&self, id: &str) -> Result<Option<DocumentRecord>> {
        // 先检查缓存
        if let Some(record) = self.cache.get(id).await {
            return Ok(Some(record));
        }

        // 从数据库读取
        let documents = self.documents.clone();
        let id_owned = id.to_string();
        
        let record = task::spawn_blocking(move || {
            if let Some(data) = documents.get(&id_owned)? {
                let record: DocumentRecord = bincode::deserialize(&data)
                    .map_err(|e| VectorDbError::storage_error(format!("无法反序列化文档: {}", e)))?;
                Ok(Some(record))
            } else {
                Ok(None)
            }
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?;

        // 如果找到，加入缓存
        if let Ok(Some(ref record)) = record {
            self.cache.insert(id.to_string(), record.clone()).await;
        }

        record
    }

    async fn search_similar(&self, query_vector: &[f32], limit: usize) -> Result<Vec<String>> {
        let vectors = self.vectors.clone();
        let query_vec = query_vector.to_vec();
        
        // 简化的向量相似度搜索（线性扫描）
        // 在实际应用中，这里应该使用HNSW或其他高效的向量索引
        let results = task::spawn_blocking(move || {
            let mut similarities: Vec<(String, f32)> = Vec::new();
            
            for item in vectors.iter() {
                let (key, value) = item?;
                let id = String::from_utf8_lossy(&key).to_string();
                
                if let Ok(stored_vector) = bincode::deserialize::<Vec<f32>>(&value) {
                    let similarity = cosine_similarity(&query_vec, &stored_vector);
                    similarities.push((id, similarity));
                }
            }
            
            // 按相似度排序
            similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            
            // 返回前limit个结果
            let result_ids: Vec<String> = similarities
                .into_iter()
                .take(limit)
                .map(|(id, _)| id)
                .collect();
                
            Ok::<Vec<String>, sled::Error>(result_ids)
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?
            .map_err(|e| VectorDbError::storage_error(format!("搜索失败: {}", e)))?;

        Ok(results)
    }

    async fn delete_document(&mut self, id: &str) -> Result<bool> {
        let documents = self.documents.clone();
        let vectors = self.vectors.clone();
        let id_owned = id.to_string();
        
        let existed = task::spawn_blocking(move || {
            let doc_existed = documents.remove(&id_owned)?.is_some();
            let vec_existed = vectors.remove(&id_owned)?.is_some();
            Ok::<bool, sled::Error>(doc_existed || vec_existed)
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?
            .map_err(|e| VectorDbError::storage_error(format!("删除失败: {}", e)))?;

        if existed {
            // 从缓存中移除
            self.cache.remove(id).await;
            // 更新统计信息
            self.update_stats(-1, -1);
        }

        Ok(existed)
    }

    async fn update_document(&mut self, record: DocumentRecord) -> Result<()> {
        // 更新操作等同于添加（覆盖）
        self.add_document(record).await
    }

    async fn list_documents(&self, offset: usize, limit: usize) -> Result<Vec<DocumentRecord>> {
        let documents = self.documents.clone();
        
        let records = task::spawn_blocking(move || {
            let mut results = Vec::new();
            let mut count = 0;
            let mut skipped = 0;
            
            for item in documents.iter() {
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                
                if count >= limit {
                    break;
                }
                
                let (_, value) = item?;
                if let Ok(record) = bincode::deserialize::<DocumentRecord>(&value) {
                    results.push(record);
                    count += 1;
                }
            }
            
            Ok::<Vec<DocumentRecord>, sled::Error>(results)
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?
            .map_err(|e| VectorDbError::storage_error(format!("列表查询失败: {}", e)))?;

        Ok(records)
    }

    async fn save(&self) -> Result<()> {
        let db = self.db.clone();
        
        // 保存统计信息
        self.save_stats().await?;
        
        // 刷新数据库
        task::spawn_blocking(move || {
            db.flush()
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?
            .map_err(|e| VectorDbError::storage_error(format!("数据库刷新失败: {}", e)))?;

        Ok(())
    }

    async fn load(&mut self) -> Result<()> {
        // 重新加载统计信息
        self.load_stats().await?;
        
        // 清空缓存以确保数据一致性
        self.cache.invalidate_all();
        
        Ok(())
    }

    async fn compact(&self) -> Result<()> {
        let documents = self.documents.clone();
        let vectors = self.vectors.clone();
        let metadata = self.metadata.clone();
        
        task::spawn_blocking(move || {
            // 压缩各个树
            documents.flush()?;
            vectors.flush()?;
            metadata.flush()?;
            Ok::<(), sled::Error>(())
        }).await
            .map_err(|e| VectorDbError::storage_error(format!("任务执行失败: {}", e)))?
            .map_err(|e| VectorDbError::storage_error(format!("压缩失败: {}", e)))?;

        Ok(())
    }

    fn stats(&self) -> DatabaseStats {
        self.stats.read().clone()
    }
}

/// 计算余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

/// 基础向量存储实现（保持向后兼容）
pub struct BasicVectorStore {
    inner: SledVectorStore,
}

impl BasicVectorStore {
    pub async fn new(data_dir: PathBuf, config: &VectorDbConfig) -> Result<Self> {
        let inner = SledVectorStore::new(data_dir, config).await?;
        Ok(Self { inner })
    }
}

#[async_trait]
impl VectorStore for BasicVectorStore {
    async fn add_document(&mut self, record: DocumentRecord) -> Result<()> {
        self.inner.add_document(record).await
    }

    async fn get_document(&self, id: &str) -> Result<Option<DocumentRecord>> {
        self.inner.get_document(id).await
    }

    async fn search_similar(&self, query_vector: &[f32], limit: usize) -> Result<Vec<String>> {
        self.inner.search_similar(query_vector, limit).await
    }

    async fn delete_document(&mut self, id: &str) -> Result<bool> {
        self.inner.delete_document(id).await
    }

    async fn update_document(&mut self, record: DocumentRecord) -> Result<()> {
        self.inner.update_document(record).await
    }

    async fn list_documents(&self, offset: usize, limit: usize) -> Result<Vec<DocumentRecord>> {
        self.inner.list_documents(offset, limit).await
    }

    async fn save(&self) -> Result<()> {
        self.inner.save().await
    }

    async fn load(&mut self) -> Result<()> {
        self.inner.load().await
    }

    async fn compact(&self) -> Result<()> {
        self.inner.compact().await
    }

    fn stats(&self) -> DatabaseStats {
        self.inner.stats()
    }
} 