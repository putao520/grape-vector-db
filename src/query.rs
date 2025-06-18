use crate::{
    types::*, 
    config::VectorDbConfig, 
    storage::VectorStore, 
    index::{HnswVectorIndex, VectorIndex},
    metrics::{MetricsCollector, QueryTimer},
    errors::{Result, VectorDbError}
};
use std::sync::Arc;
use parking_lot::{RwLock, Mutex};
use std::collections::HashMap;

/// 查询引擎
pub struct QueryEngine {
    config: VectorDbConfig,
    hnsw_index: Arc<Mutex<HnswVectorIndex>>,
    metrics: Arc<MetricsCollector>,
}

impl QueryEngine {
    pub fn new(config: &VectorDbConfig, metrics: Arc<MetricsCollector>) -> Result<Self> {
        // 创建HNSW索引
        let hnsw_index = Arc::new(Mutex::new(HnswVectorIndex::with_config(
            config.hnsw.clone(),
            config.vector_dimension,
        )));

        Ok(Self {
            config: config.clone(),
            hnsw_index,
            metrics,
        })
    }

    /// 添加文档到索引
    pub async fn add_document(&self, record: &DocumentRecord) -> Result<()> {
        let _timer = QueryTimer::new(self.metrics.clone());

        // 添加到向量索引
        self.hnsw_index.lock().add_vector(record.id.clone(), record.embedding.clone())?;

        Ok(())
    }

    /// 删除文档
    pub async fn remove_document(&self, document_id: &str) -> Result<bool> {
        let _timer = QueryTimer::new(self.metrics.clone());

        // 从向量索引删除
        let removed_from_vector = self.hnsw_index.lock().remove_vector(document_id)?;

        Ok(removed_from_vector)
    }

    /// 混合搜索：向量相似度 + 简单文本搜索
    pub async fn search<S: VectorStore + ?Sized>(
        &self,
        store: &S,
        query_vector: Option<&[f32]>,
        query_text: Option<&str>,
        limit: usize,
        vector_weight: f32,
        text_weight: f32,
    ) -> Result<Vec<SearchResult>> {
        let _timer = QueryTimer::new(self.metrics.clone());

        let mut vector_results = HashMap::new();
        let mut text_results_map = HashMap::new();

        // 向量搜索
        if let Some(vector) = query_vector {
            let results = self.hnsw_index.lock().search(vector, limit * 2)?;
            for (i, (document_id, similarity)) in results.iter().enumerate() {
                let score = similarity * vector_weight * (1.0 - i as f32 / results.len() as f32);
                vector_results.insert(document_id.clone(), score);
            }
        }


        // 简单文本搜索（使用分页避免内存问题）
        if let Some(text) = query_text {
            let text_lower = text.to_lowercase();
            let mut text_results = Vec::new();
            let page_size = 500;
            let mut offset = 0;
            let max_docs = 5000; // 限制最大搜索文档数
            
            while offset < max_docs && text_results.len() < limit {
                let doc_ids = store.list_document_ids(offset, page_size).await?;
                
                if doc_ids.is_empty() {
                    break;
                }
                
                for doc_id in doc_ids {
                    if let Some(doc) = store.get_document(&doc_id).await? {
                    let content_lower = doc.content.to_lowercase();
                    let title_lower = doc.title.to_lowercase();
                    

                    // 简单的文本匹配评分
                    let mut score = 0.0;
                    if title_lower.contains(&text_lower) {
                        score += 2.0; // 标题匹配权重更高
                    }
                    if content_lower.contains(&text_lower) {
                        score += 1.0;
                    }
                    
                    if score > 0.0 {
                        text_results.push((doc.id.clone(), score));
                    }
                    }
                }
                
                offset += page_size;

            }
            
            // 对文本搜索结果进行排序和权重计算
            text_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (i, (doc_id, score)) in text_results.iter().enumerate() {
                let weighted_score = score * text_weight * (1.0 - i as f32 / text_results.len().max(1) as f32);
                text_results_map.insert(doc_id.clone(), weighted_score);
            }
        }

        // 合并结果
        let mut combined_scores = HashMap::new();
        
        // 添加向量搜索结果
        for (doc_id, score) in vector_results {
            combined_scores.insert(doc_id, score);
        }
        
        // 添加或合并文本搜索结果
        for (doc_id, score) in text_results_map {
            *combined_scores.entry(doc_id).or_insert(0.0) += score;
        }

        // 按分数排序并获取文档详情
        let mut sorted_results: Vec<_> = combined_scores.into_iter().collect();
        sorted_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut final_results = Vec::new();
        for (doc_id, score) in sorted_results.into_iter().take(limit) {
            if let Some(doc) = store.get_document(&doc_id).await? {
                let snippet = if let Some(query) = query_text {
                    Some(vec![self.extract_snippet(&doc.content, Some(query))])
                } else {
                    None
                };
                
                let result = SearchResult {
                    document: doc,
                    score,
                    relevance_score: Some(score),
                    matched_snippets: snippet,
                };
                final_results.push(result);
            }
        }

        Ok(final_results)
    }

    /// 纯向量搜索
    pub async fn vector_search<S: VectorStore + ?Sized>(
        &self,
        store: &S,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        self.search(store, Some(query_vector), None, limit, 1.0, 0.0).await
    }

    /// 纯文本搜索
    pub async fn text_search<S: VectorStore + ?Sized>(
        &self,
        store: &S,
        query_text: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        self.search(store, None, Some(query_text), limit, 0.0, 1.0).await
    }

    /// 提取内容摘要
    fn extract_snippet(&self, content: &str, query_text: Option<&str>) -> String {
        let max_length = 200;
        
        if let Some(query) = query_text {
            // 尝试找到包含查询词的片段
            let query_lower = query.to_lowercase();
            let content_lower = content.to_lowercase();
            
            if let Some(pos) = content_lower.find(&query_lower) {
                // 使用字符索引而不是字节索引来避免UTF-8边界问题
                let chars: Vec<char> = content.chars().collect();
                let content_lower_chars: Vec<char> = content_lower.chars().collect();
                
                // 找到查询词在字符数组中的位置
                let mut char_pos = 0;
                for (i, window) in content_lower_chars.windows(query.chars().count()).enumerate() {
                    let window_str: String = window.iter().collect();
                    if window_str == query_lower {
                        char_pos = i;
                        break;
                    }
                }
                
                let start_char = char_pos.saturating_sub(50);
                let end_char = (char_pos + query.chars().count() + 150).min(chars.len());
                
                let snippet: String = chars[start_char..end_char].iter().collect();
                
                if snippet.chars().count() > max_length {
                    let truncated: String = snippet.chars().take(max_length).collect();
                    format!("...{}", truncated)
                } else if start_char > 0 {
                    format!("...{}", snippet)
                } else {
                    snippet
                }
            } else {
                // 如果没找到查询词，返回开头
                content.chars().take(max_length).collect()
            }
        } else {
            // 没有查询文本，返回开头
            content.chars().take(max_length).collect()
        }
    }

    /// 重建索引
    pub async fn rebuild_index(&self) -> Result<()> {
        let start_time = std::time::Instant::now();
        
        // 重建向量索引
        self.hnsw_index.lock().build_index()?;
        
        let elapsed = start_time.elapsed();
        self.metrics.record_index_build_time(elapsed.as_secs_f64() * 1000.0);
        
        Ok(())
    }

    /// 获取索引统计信息
    pub fn get_index_stats(&self) -> IndexStats {
        let hnsw_stats = self.hnsw_index.lock().get_stats();
        IndexStats {
            point_count: hnsw_stats.vector_count, // Map vector_count to point_count
            dimension: hnsw_stats.dimension,
            is_built: !hnsw_stats.index_type.is_empty(), // Derive is_built from index_type
            memory_usage_mb: hnsw_stats.memory_usage as f64 / 1024.0 / 1024.0, // Convert bytes to MB
        }
    }

    /// 保存索引到文件
    pub async fn save_index(&self, _path: &std::path::Path) -> Result<()> {
        // TODO: Implement index persistence
        Ok(())
    }

    /// 从文件加载索引
    pub async fn load_index(&self, _path: &std::path::Path) -> Result<()> {
        // TODO: Implement index loading
        Ok(())
    }
}

/// 索引统计信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexStats {
    pub point_count: usize,
    pub dimension: usize,
    pub is_built: bool,
    pub memory_usage_mb: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::BasicVectorStore;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_query_engine() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();
        let metrics = Arc::new(MetricsCollector::new());
        
        let engine = QueryEngine::new(&config, metrics).unwrap();
        let mut store = BasicVectorStore::new(temp_dir.path().to_str().unwrap()).unwrap();

        // 添加测试文档
        let doc = DocumentRecord {
            id: "test1".to_string(),
            title: "测试文档".to_string(),
            content: "这是一个测试文档的内容".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            package_name: "test_package".to_string(),
            doc_type: "test".to_string(),
            metadata: std::collections::HashMap::new(),
            language: "zh".to_string(),
            version: "1".to_string(),
            vector: Some(vec![1.0, 0.0, 0.0]),
            sparse_representation: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // 将DocumentRecord转换为Document
        let document = Document {
            id: doc.id.clone(),
            content: doc.content.clone(),
            title: Some(doc.title.clone()),
            language: Some(doc.language.clone()),
            package_name: Some(doc.package_name.clone()),
            version: Some(doc.version.clone()),
            doc_type: Some(doc.doc_type.clone()),
            vector: doc.vector.clone(),
            sparse_representation: doc.sparse_representation.clone(),
            metadata: doc.metadata.clone(),
        };

        store.insert_document(document).await.unwrap();
        engine.add_document(&doc).await.unwrap();
        engine.rebuild_index().await.unwrap();

        // 测试向量搜索
        let results = engine.vector_search(&store, &[1.0, 0.1, 0.0], 5).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].document.id, "test1");
    }
} 
