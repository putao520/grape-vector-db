use crate::types::{SearchResult, VectorDbError, QueryRequest, QueryResponse};
use crate::storage::{VectorStore};
use crate::index::{VectorIndex};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// 查询引擎配置
#[derive(Debug, Clone)]
pub struct QueryEngineConfig {
    /// 默认搜索结果数量
    pub default_limit: usize,
    /// 默认相似度阈值
    pub default_threshold: f32,
    /// 混合搜索中文本搜索的权重
    pub text_weight: f32,
    /// 是否启用缓存
    pub enable_cache: bool,
    /// 缓存大小
    pub cache_size: usize,
}

impl Default for QueryEngineConfig {
    fn default() -> Self {
        Self {
            default_limit: 10,
            default_threshold: 0.7,
            text_weight: 0.3,
            enable_cache: true,
            cache_size: 1000,
        }
    }
}

/// 查询引擎
#[derive(Clone)]
pub struct QueryEngine {
    storage: Arc<RwLock<dyn VectorStore>>,
    vector_index: Arc<RwLock<dyn VectorIndex>>,
    config: QueryEngineConfig,
    cache: Option<moka::future::Cache<String, Vec<SearchResult>>>,
}

impl QueryEngine {
    /// 创建新的查询引擎
    pub fn new(
        storage: Arc<RwLock<dyn VectorStore>>,
        vector_index: Arc<RwLock<dyn VectorIndex>>,
        config: QueryEngineConfig,
    ) -> Self {
        let cache = if config.enable_cache {
            Some(
                moka::future::Cache::builder()
                    .max_capacity(config.cache_size as u64)
                    .build()
            )
        } else {
            None
        };

        Self {
            storage,
            vector_index,
            config,
            cache,
        }
    }

    /// 使用默认配置创建查询引擎
    pub fn with_defaults(
        storage: Arc<RwLock<dyn VectorStore>>,
        vector_index: Arc<RwLock<dyn VectorIndex>>,
    ) -> Self {
        Self::new(storage, vector_index, QueryEngineConfig::default())
    }

    /// 执行查询
    pub async fn execute_query(&self, request: QueryRequest) -> Result<QueryResponse, VectorDbError> {
        match request {
            QueryRequest::VectorSearch { query_vector, limit, threshold } => {
                let results = self.vector_search(&query_vector, Some(limit), threshold).await?;
                Ok(QueryResponse::SearchResults(results))
            }
            QueryRequest::TextSearch { query, limit, filters } => {
                let results = self.text_search(&query, Some(limit), filters).await?;
                Ok(QueryResponse::SearchResults(results))
            }
            QueryRequest::HybridSearch { query, query_vector, limit, alpha } => {
                let results = self.hybrid_search(&query, query_vector.as_deref(), Some(limit), alpha).await?;
                Ok(QueryResponse::SearchResults(results))
            }
        }
    }

    /// 向量搜索
    pub async fn vector_search(
        &self,
        query_vector: &[f32],
        limit: Option<usize>,
        threshold: Option<f32>,
    ) -> Result<Vec<SearchResult>, VectorDbError> {
        let limit = limit.unwrap_or(self.config.default_limit);
        let threshold = threshold.or(Some(self.config.default_threshold));

        // 检查缓存
        let cache_key = format!("vector:{:?}:{}:{:?}", query_vector, limit, threshold);
        if let Some(ref cache) = self.cache {
            if let Some(cached_results) = cache.get(&cache_key).await {
                return Ok(cached_results);
            }
        }

        let storage = self.storage.read().await;
        let results = storage.vector_search(query_vector, limit, threshold).await?;

        // 缓存结果
        if let Some(ref cache) = self.cache {
            cache.insert(cache_key, results.clone()).await;
        }

        Ok(results)
    }

    /// 文本搜索
    pub async fn text_search(
        &self,
        query: &str,
        limit: Option<usize>,
        filters: Option<HashMap<String, String>>,
    ) -> Result<Vec<SearchResult>, VectorDbError> {
        let limit = limit.unwrap_or(self.config.default_limit);

        // 检查缓存
        let cache_key = format!("text:{}:{}:{:?}", query, limit, filters);
        if let Some(ref cache) = self.cache {
            if let Some(cached_results) = cache.get(&cache_key).await {
                return Ok(cached_results);
            }
        }

        let storage = self.storage.read().await;
        let results = storage.text_search(query, limit, filters).await?;

        // 缓存结果
        if let Some(ref cache) = self.cache {
            cache.insert(cache_key, results.clone()).await;
        }

        Ok(results)
    }

    /// 混合搜索
    pub async fn hybrid_search(
        &self,
        query: &str,
        query_vector: Option<&[f32]>,
        limit: Option<usize>,
        alpha: Option<f32>,
    ) -> Result<Vec<SearchResult>, VectorDbError> {
        let limit = limit.unwrap_or(self.config.default_limit);
        let alpha = alpha.unwrap_or(self.config.text_weight);

        // 检查缓存
        let cache_key = format!("hybrid:{}:{:?}:{}:{}", query, query_vector, limit, alpha);
        if let Some(ref cache) = self.cache {
            if let Some(cached_results) = cache.get(&cache_key).await {
                return Ok(cached_results);
            }
        }

        let storage = self.storage.read().await;
        let results = storage.hybrid_search(query, query_vector, limit, alpha).await?;

        // 缓存结果
        if let Some(ref cache) = self.cache {
            cache.insert(cache_key, results.clone()).await;
        }

        Ok(results)
    }

    /// 清除缓存
    pub async fn clear_cache(&self) {
        if let Some(ref cache) = self.cache {
            cache.invalidate_all();
        }
    }

    /// 获取缓存统计信息
    pub async fn get_cache_stats(&self) -> Option<(u64, u64)> {
        if let Some(ref cache) = self.cache {
            Some((cache.entry_count(), cache.weighted_size()))
        } else {
            None
        }
    }

    /// 更新配置
    pub fn update_config(&mut self, config: QueryEngineConfig) {
        self.config = config;
    }

    /// 获取当前配置
    pub fn get_config(&self) -> &QueryEngineConfig {
        &self.config
    }
}

/// 查询优化器
pub struct QueryOptimizer {
    optimization_rules: Vec<OptimizationRule>,
}

/// 优化规则
#[derive(Debug, Clone)]
pub enum OptimizationRule {
    /// 限制最大结果数量
    LimitMaxResults { max_limit: usize },
    /// 设置最小相似度阈值
    MinSimilarityThreshold { min_threshold: f32 },
    /// 查询重写
    QueryRewrite { patterns: Vec<(String, String)> },
}

impl QueryOptimizer {
    /// 创建新的查询优化器
    pub fn new() -> Self {
        Self {
            optimization_rules: vec![
                OptimizationRule::LimitMaxResults { max_limit: 100 },
                OptimizationRule::MinSimilarityThreshold { min_threshold: 0.1 },
            ],
        }
    }

    /// 添加优化规则
    pub fn add_rule(&mut self, rule: OptimizationRule) {
        self.optimization_rules.push(rule);
    }

    /// 优化查询请求
    pub fn optimize_query(&self, mut request: QueryRequest) -> QueryRequest {
        for rule in &self.optimization_rules {
            request = self.apply_rule(request, rule);
        }
        request
    }

    /// 应用单个优化规则
    fn apply_rule(&self, request: QueryRequest, rule: &OptimizationRule) -> QueryRequest {
        match rule {
            OptimizationRule::LimitMaxResults { max_limit } => {
                match request {
                    QueryRequest::VectorSearch { query_vector, limit, threshold } => {
                        let new_limit = limit.min(*max_limit);
                        QueryRequest::VectorSearch { query_vector, limit: new_limit, threshold }
                    }
                    QueryRequest::TextSearch { query, limit, filters } => {
                        let new_limit = limit.min(*max_limit);
                        QueryRequest::TextSearch { query, limit: new_limit, filters }
                    }
                    QueryRequest::HybridSearch { query, query_vector, limit, alpha } => {
                        let new_limit = limit.min(*max_limit);
                        QueryRequest::HybridSearch { query, query_vector, limit: new_limit, alpha }
                    }
                }
            }
            OptimizationRule::MinSimilarityThreshold { min_threshold } => {
                match request {
                    QueryRequest::VectorSearch { query_vector, limit, threshold } => {
                        let new_threshold = threshold.map(|t| t.max(*min_threshold)).or(Some(*min_threshold));
                        QueryRequest::VectorSearch { query_vector, limit, threshold: new_threshold }
                    }
                    _ => request,
                }
            }
            OptimizationRule::QueryRewrite { patterns } => {
                match request {
                    QueryRequest::TextSearch { mut query, limit, filters } => {
                        for (pattern, replacement) in patterns {
                            query = query.replace(pattern, replacement);
                        }
                        QueryRequest::TextSearch { query, limit, filters }
                    }
                    QueryRequest::HybridSearch { mut query, query_vector, limit, alpha } => {
                        for (pattern, replacement) in patterns {
                            query = query.replace(pattern, replacement);
                        }
                        QueryRequest::HybridSearch { query, query_vector, limit, alpha }
                    }
                    _ => request,
                }
            }
        }
    }
}

impl Default for QueryOptimizer {
    fn default() -> Self {
        Self::new()
    }
} 