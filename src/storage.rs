use crate::types::{Document, DocumentRecord, SearchResult, VectorDbError, VectorDbStats};
use async_trait::async_trait;
use sled::Db;
use std::collections::HashMap;
use std::path::Path;

/// 向量存储特征
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// 插入文档
    async fn insert_document(&mut self, document: Document) -> Result<String, VectorDbError>;

    /// 批量插入文档
    async fn batch_insert_documents(
        &mut self,
        documents: Vec<Document>,
    ) -> Result<Vec<String>, VectorDbError>;

    /// 获取文档
    async fn get_document(&self, id: &str) -> Result<Option<DocumentRecord>, VectorDbError>;

    /// 删除文档
    async fn delete_document(&mut self, id: &str) -> Result<bool, VectorDbError>;

    /// 更新文档
    async fn update_document(
        &mut self,
        id: &str,
        document: Document,
    ) -> Result<bool, VectorDbError>;

    /// 向量搜索
    async fn vector_search(
        &self,
        query_vector: &[f32],
        limit: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<SearchResult>, VectorDbError>;

    /// 文本搜索
    async fn text_search(
        &self,
        query: &str,
        limit: usize,
        filters: Option<HashMap<String, String>>,
    ) -> Result<Vec<SearchResult>, VectorDbError>;

    /// 混合搜索
    async fn hybrid_search(
        &self,
        query: &str,
        query_vector: Option<&[f32]>,
        limit: usize,
        alpha: f32,
    ) -> Result<Vec<SearchResult>, VectorDbError>;

    /// 获取统计信息
    async fn get_stats(&self) -> Result<VectorDbStats, VectorDbError>;

    /// 优化存储
    async fn optimize(&mut self) -> Result<(), VectorDbError>;

    /// 备份数据
    async fn backup(&self, path: &Path) -> Result<(), VectorDbError>;

    /// 恢复数据
    async fn restore(&mut self, path: &Path) -> Result<(), VectorDbError>;

    /// 清空所有数据
    async fn clear(&mut self) -> Result<(), VectorDbError>;

    /// 获取文档数量
    async fn count_documents(&self) -> Result<usize, VectorDbError>;

    /// 列出所有文档ID
    async fn list_document_ids(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, VectorDbError>;

    /// 检查文档是否存在
    async fn document_exists(&self, id: &str) -> Result<bool, VectorDbError>;

    /// 获取文档元数据
    async fn get_document_metadata(
        &self,
        id: &str,
    ) -> Result<Option<HashMap<String, String>>, VectorDbError>;

    /// 更新文档元数据
    async fn update_document_metadata(
        &mut self,
        id: &str,
        metadata: HashMap<String, String>,
    ) -> Result<bool, VectorDbError>;

    /// 按条件搜索文档
    async fn search_by_metadata(
        &self,
        filters: HashMap<String, String>,
        limit: usize,
    ) -> Result<Vec<DocumentRecord>, VectorDbError>;
}

/// 基础向量存储实现
pub struct BasicVectorStore {
    db: Db,
}

impl BasicVectorStore {
    /// 创建新的向量存储实例
    pub fn new(db_path: &str) -> Result<Self, VectorDbError> {
        let db = sled::open(db_path).map_err(|e| VectorDbError::StorageError(e.to_string()))?;

        Ok(Self { db })
    }

    /// 从现有数据库创建实例
    pub fn from_db(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl VectorStore for BasicVectorStore {
    async fn insert_document(&mut self, document: Document) -> Result<String, VectorDbError> {
        let id = if document.id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            document.id.clone()
        };

        let now = chrono::Utc::now();
        let record = DocumentRecord {
            id: id.clone(),
            content: document.content,
            title: document.title.unwrap_or_default(),
            language: document.language.unwrap_or_default(),
            package_name: document.package_name.unwrap_or_default(),
            version: document.version.unwrap_or_default(),
            doc_type: document.doc_type.unwrap_or_default(),
            vector: document.vector,
            metadata: document.metadata,
            embedding: Vec::new(), // 需要后续生成
            sparse_representation: None,
            created_at: now,
            updated_at: now,
        };

        let key = format!("doc:{}", id);
        let value = postcard::to_allocvec(&record)
            .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;

        self.db
            .insert(key, value)
            .map_err(|e| VectorDbError::StorageError(e.to_string()))?;

        Ok(id)
    }

    async fn batch_insert_documents(
        &mut self,
        documents: Vec<Document>,
    ) -> Result<Vec<String>, VectorDbError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let now = chrono::Utc::now();
        let mut batch_data = Vec::new();
        let mut ids = Vec::new();

        // 准备批量数据
        for document in documents {
            let id = if document.id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                document.id.clone()
            };

            let record = DocumentRecord {
                id: id.clone(),
                content: document.content,
                title: document.title.unwrap_or_default(),
                language: document.language.unwrap_or_default(),
                package_name: document.package_name.unwrap_or_default(),
                version: document.version.unwrap_or_default(),
                doc_type: document.doc_type.unwrap_or_default(),
                vector: document.vector,
                metadata: document.metadata,
                embedding: Vec::new(),
                sparse_representation: None,
                created_at: now,
                updated_at: now,
            };

            let key = format!("doc:{}", id);
            let value = postcard::to_allocvec(&record)
                .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;

            batch_data.push((key, value));
            ids.push(id);
        }

        // 使用 Sled 的批量操作
        let mut batch = sled::Batch::default();
        for (key, value) in batch_data {
            batch.insert(key.as_bytes(), value);
        }

        self.db
            .apply_batch(batch)
            .map_err(|e| VectorDbError::StorageError(e.to_string()))?;

        Ok(ids)
    }

    async fn get_document(&self, id: &str) -> Result<Option<DocumentRecord>, VectorDbError> {
        let key = format!("doc:{}", id);
        match self.db.get(&key) {
            Ok(Some(value)) => {
                let record = postcard::from_bytes::<DocumentRecord>(&value)
                    .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;
                Ok(Some(record))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(VectorDbError::StorageError(e.to_string())),
        }
    }

    async fn delete_document(&mut self, id: &str) -> Result<bool, VectorDbError> {
        let key = format!("doc:{}", id);
        match self.db.remove(&key) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(VectorDbError::StorageError(e.to_string())),
        }
    }

    async fn update_document(
        &mut self,
        id: &str,
        document: Document,
    ) -> Result<bool, VectorDbError> {
        let key = format!("doc:{}", id);

        // 检查文档是否存在
        if let Ok(Some(existing_data)) = self.db.get(&key) {
            let mut existing_record = postcard::from_bytes::<DocumentRecord>(&existing_data)
                .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;

            // 更新字段
            existing_record.content = document.content;
            existing_record.title = document.title.unwrap_or(existing_record.title);
            existing_record.language = document.language.unwrap_or(existing_record.language);
            existing_record.package_name = document
                .package_name
                .unwrap_or(existing_record.package_name);
            existing_record.version = document.version.unwrap_or(existing_record.version);
            existing_record.doc_type = document.doc_type.unwrap_or(existing_record.doc_type);
            existing_record.vector = document.vector.or(existing_record.vector);
            existing_record.metadata = document.metadata;
            existing_record.updated_at = chrono::Utc::now();

            let value = postcard::to_allocvec(&existing_record)
                .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;

            self.db
                .insert(key, value)
                .map_err(|e| VectorDbError::StorageError(e.to_string()))?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn vector_search(
        &self,
        query_vector: &[f32],
        limit: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<SearchResult>, VectorDbError> {
        let mut results = Vec::new();

        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| VectorDbError::StorageError(e.to_string()))?;

            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with("doc:") {
                    if let Ok(record) = postcard::from_bytes::<DocumentRecord>(&value) {
                        if let Some(ref vector) = record.vector {
                            let similarity = cosine_similarity(query_vector, vector);

                            if let Some(thresh) = threshold {
                                if similarity < thresh {
                                    continue;
                                }
                            }

                            results.push(SearchResult {
                                document: record,
                                score: similarity,
                                relevance_score: Some(similarity),
                                matched_snippets: None,
                            });
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    async fn text_search(
        &self,
        query: &str,
        limit: usize,
        _filters: Option<HashMap<String, String>>,
    ) -> Result<Vec<SearchResult>, VectorDbError> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| VectorDbError::StorageError(e.to_string()))?;

            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with("doc:") {
                    if let Ok(record) = postcard::from_bytes::<DocumentRecord>(&value) {
                        let content_lower = record.content.to_lowercase();
                        let title_lower = record.title.to_lowercase();

                        let mut score = 0.0;
                        if content_lower.contains(&query_lower) {
                            score += 0.7;
                        }
                        if title_lower.contains(&query_lower) {
                            score += 0.3;
                        }

                        if score > 0.0 {
                            results.push(SearchResult {
                                document: record,
                                score,
                                relevance_score: Some(score),
                                matched_snippets: None,
                            });
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    async fn hybrid_search(
        &self,
        query: &str,
        query_vector: Option<&[f32]>,
        limit: usize,
        alpha: f32,
    ) -> Result<Vec<SearchResult>, VectorDbError> {
        let text_results = self.text_search(query, limit * 2, None).await?;

        if let Some(vector) = query_vector {
            let vector_results = self.vector_search(vector, limit * 2, None).await?;

            // 合并结果
            let mut combined_results = HashMap::new();

            for result in text_results {
                combined_results.insert(result.document.id.clone(), (result, alpha));
            }

            for result in vector_results {
                if let Some((mut existing, text_weight)) =
                    combined_results.remove(&result.document.id)
                {
                    existing.score = text_weight * existing.score + (1.0 - alpha) * result.score;
                    combined_results.insert(result.document.id.clone(), (existing, 0.0));
                } else {
                    combined_results.insert(result.document.id.clone(), (result, 0.0));
                }
            }

            let mut final_results: Vec<SearchResult> = combined_results
                .into_values()
                .map(|(result, _)| result)
                .collect();
            final_results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            final_results.truncate(limit);

            Ok(final_results)
        } else {
            Ok(text_results)
        }
    }

    async fn get_stats(&self) -> Result<VectorDbStats, VectorDbError> {
        let mut document_count = 0;
        let mut total_size = 0;

        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| VectorDbError::StorageError(e.to_string()))?;
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with("doc:") {
                    document_count += 1;
                    total_size += value.len();
                }
            }
        }

        Ok(VectorDbStats {
            total_documents: document_count,
            total_vectors: document_count, // 假设每个文档都有向量
            index_size_bytes: total_size as u64,
            memory_usage_bytes: total_size as u64,
            last_optimization: None,
        })
    }

    async fn optimize(&mut self) -> Result<(), VectorDbError> {
        // Sled 自动优化，这里可以添加其他优化逻辑
        Ok(())
    }

    async fn backup(&self, path: &Path) -> Result<(), VectorDbError> {
        // 简单实现：导出所有数据到文件
        let mut backup_data = Vec::new();
        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| VectorDbError::StorageError(e.to_string()))?;
            backup_data.push((key.to_vec(), value.to_vec()));
        }

        let serialized = postcard::to_allocvec(&backup_data)
            .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;
        std::fs::write(path, serialized).map_err(|e| VectorDbError::StorageError(e.to_string()))?;
        Ok(())
    }

    async fn restore(&mut self, path: &Path) -> Result<(), VectorDbError> {
        // 简单实现：从备份文件恢复数据
        let backup_data =
            std::fs::read(path).map_err(|e| VectorDbError::StorageError(e.to_string()))?;

        let data: Vec<(Vec<u8>, Vec<u8>)> = postcard::from_bytes(&backup_data)
            .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;

        // 清空现有数据
        self.db
            .clear()
            .map_err(|e| VectorDbError::StorageError(e.to_string()))?;

        // 恢复数据
        for (key, value) in data {
            self.db
                .insert(key, value)
                .map_err(|e| VectorDbError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    async fn clear(&mut self) -> Result<(), VectorDbError> {
        self.db
            .clear()
            .map_err(|e| VectorDbError::StorageError(e.to_string()))?;
        Ok(())
    }

    async fn count_documents(&self) -> Result<usize, VectorDbError> {
        let mut count = 0;
        for item in self.db.iter() {
            let (key, _) = item.map_err(|e| VectorDbError::StorageError(e.to_string()))?;
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with("doc:") {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    async fn list_document_ids(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, VectorDbError> {
        let mut ids = Vec::new();
        let mut current = 0;

        for item in self.db.iter() {
            let (key, _) = item.map_err(|e| VectorDbError::StorageError(e.to_string()))?;
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with("doc:") {
                    if current >= offset {
                        if ids.len() >= limit {
                            break;
                        }
                        // 安全地提取文档ID，如果前缀不匹配则跳过
                        if let Some(doc_id) = key_str.strip_prefix("doc:") {
                            ids.push(doc_id.to_string());
                        }
                    }
                    current += 1;
                }
            }
        }

        Ok(ids)
    }

    async fn document_exists(&self, id: &str) -> Result<bool, VectorDbError> {
        let key = format!("doc:{}", id);
        Ok(self
            .db
            .contains_key(&key)
            .map_err(|e| VectorDbError::StorageError(e.to_string()))?)
    }

    async fn get_document_metadata(
        &self,
        id: &str,
    ) -> Result<Option<HashMap<String, String>>, VectorDbError> {
        if let Some(record) = self.get_document(id).await? {
            Ok(Some(record.metadata))
        } else {
            Ok(None)
        }
    }

    async fn update_document_metadata(
        &mut self,
        id: &str,
        metadata: HashMap<String, String>,
    ) -> Result<bool, VectorDbError> {
        let key = format!("doc:{}", id);

        if let Ok(Some(existing_data)) = self.db.get(&key) {
            let mut existing_record = postcard::from_bytes::<DocumentRecord>(&existing_data)
                .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;

            existing_record.metadata = metadata;
            existing_record.updated_at = chrono::Utc::now();

            let value = postcard::to_allocvec(&existing_record)
                .map_err(|e| VectorDbError::SerializationError(e.to_string()))?;

            self.db
                .insert(key, value)
                .map_err(|e| VectorDbError::StorageError(e.to_string()))?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn search_by_metadata(
        &self,
        filters: HashMap<String, String>,
        limit: usize,
    ) -> Result<Vec<DocumentRecord>, VectorDbError> {
        let mut results = Vec::new();

        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| VectorDbError::StorageError(e.to_string()))?;

            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with("doc:") {
                    if let Ok(record) = postcard::from_bytes::<DocumentRecord>(&value) {
                        let mut matches = true;
                        for (filter_key, filter_value) in &filters {
                            if let Some(metadata_value) = record.metadata.get(filter_key) {
                                if metadata_value != filter_value {
                                    matches = false;
                                    break;
                                }
                            } else {
                                matches = false;
                                break;
                            }
                        }

                        if matches {
                            results.push(record);
                            if results.len() >= limit {
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

/// 计算两个向量的余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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
