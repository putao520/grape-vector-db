use crate::{types::*, config::VectorDbConfig, storage::VectorStore, errors::Result};

/// 查询引擎
pub struct QueryEngine {
    _config: VectorDbConfig,
}

impl QueryEngine {
    pub fn new(config: &VectorDbConfig) -> Self {
        Self {
            _config: config.clone(),
        }
    }

    pub async fn search<S: VectorStore + ?Sized>(
        &self,
        store: &S,
        _query_vector: &[f32],
        query_text: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        // 简化实现：获取相似文档ID并构造搜索结果
        let doc_ids = store.search_similar(_query_vector, limit).await?;
        let mut results = Vec::new();

        for doc_id in doc_ids {
            if let Some(doc) = store.get_document(&doc_id).await? {
                let result = SearchResult {
                    document_id: doc.id.clone(),
                    title: doc.title.clone(),
                    content_snippet: doc.content.chars().take(200).collect(),
                    similarity_score: 0.8, // 模拟相似度分数
                    package_name: doc.package_name.clone(),
                    doc_type: doc.doc_type.clone(),
                    metadata: doc.metadata.clone(),
                };
                results.push(result);
            }
        }

        // 简单的文本匹配过滤
        if !query_text.is_empty() {
            results.retain(|r| r.content_snippet.to_lowercase().contains(&query_text.to_lowercase()));
        }

        Ok(results)
    }
} 
